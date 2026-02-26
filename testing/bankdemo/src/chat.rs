use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock};

use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status, Streaming};

use crate::auth;
use crate::pb;
use crate::pb::support_server::Support;

type SessionResult = (Arc<RwLock<Session>>, mpsc::Receiver<Option<pb::ChatEntry>>);

pub struct ChatService {
    state: Arc<Mutex<ChatState>>,
}

impl ChatService {
    pub fn new() -> Self {
        ChatService {
            state: Arc::new(Mutex::new(ChatState {
                sessions: HashMap::new(),
                awaiting_agent: Vec::new(),
                last_session: 0,
            })),
        }
    }
}

struct ChatState {
    sessions: HashMap<String, Arc<RwLock<Session>>>,
    awaiting_agent: Vec<String>,
    last_session: i32,
}

struct Session {
    id: String,
    customer_name: String,
    history: Vec<pb::ChatEntry>,
    active: bool,
    customer_tx: Option<mpsc::Sender<Option<pb::ChatEntry>>>,
    agent_txs: HashMap<String, mpsc::Sender<Option<pb::ChatEntry>>>,
}

impl Session {
    fn copy_session_proto(&self) -> pb::Session {
        pb::Session {
            session_id: self.id.clone(),
            customer_name: self.customer_name.clone(),
            history: self.history.clone(),
        }
    }
}

fn now() -> Timestamp {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    Timestamp {
        seconds: dur.as_secs() as i64,
        nanos: dur.subsec_nanos() as i32,
    }
}

#[tonic::async_trait]
impl Support for ChatService {
    type ChatCustomerStream =
        Pin<Box<dyn futures_core::Stream<Item = Result<pb::ChatCustomerResponse, Status>> + Send>>;

    async fn chat_customer(
        &self,
        request: Request<Streaming<pb::ChatCustomerRequest>>,
    ) -> Result<Response<Self::ChatCustomerStream>, Status> {
        let cust = auth::get_customer(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let mut in_stream = request.into_inner();
        let (out_tx, out_rx) = mpsc::channel::<Result<pb::ChatCustomerResponse, Status>>(32);
        let state = Arc::clone(&self.state);

        tokio::spawn(async move {
            let mut current_session: Option<Arc<RwLock<Session>>> = None;
            let mut listener_rx: Option<mpsc::Receiver<Option<pb::ChatEntry>>> = None;

            // Cleanup function equivalent
            let cleanup = |sess: &Option<Arc<RwLock<Session>>>, _cust: &str| {
                if let Some(ref sess_lock) = sess {
                    let mut s = sess_lock.write().unwrap();
                    s.customer_tx = None;
                }
            };

            loop {
                tokio::select! {
                    // Receive from client
                    msg = in_stream.next() => {
                        match msg {
                            None => {
                                // Stream closed (EOF)
                                cleanup(&current_session, &cust);
                                break;
                            }
                            Some(Err(_)) => {
                                cleanup(&current_session, &cust);
                                break;
                            }
                            Some(Ok(req)) => {
                                match req.req {
                                    Some(pb::chat_customer_request::Req::Init(init)) => {
                                        if let Some(sess) = &current_session {
                                            let id = sess.read().unwrap().id.clone();
                                            let _ = out_tx.send(Err(Status::failed_precondition(
                                                format!("already called init, currently in chat session {:?}", id)
                                            ))).await;
                                            cleanup(&current_session, &cust);
                                            break;
                                        }

                                        let session_id = init.resume_session_id;
                                        let (sess, rx) = if session_id.is_empty() {
                                            new_session(&state, &cust)
                                        } else {
                                            match resume_session(&state, &cust, &session_id) {
                                                Some(result) => result,
                                                None => {
                                                    let _ = out_tx.send(Err(Status::failed_precondition(
                                                        format!("cannot resume session {:?}; it is not an open session", session_id)
                                                    ))).await;
                                                    break;
                                                }
                                            }
                                        };

                                        let session_proto = sess.read().unwrap().copy_session_proto();
                                        let _ = out_tx.send(Ok(pb::ChatCustomerResponse {
                                            resp: Some(pb::chat_customer_response::Resp::Session(session_proto)),
                                        })).await;

                                        current_session = Some(sess);
                                        listener_rx = Some(rx);
                                    }
                                    Some(pb::chat_customer_request::Req::Msg(msg_text)) => {
                                        if current_session.is_none() {
                                            let _ = out_tx.send(Err(Status::failed_precondition(
                                                "never called init, no chat session for message"
                                            ))).await;
                                            break;
                                        }

                                        let entry = pb::ChatEntry {
                                            date: Some(now()),
                                            entry: Some(pb::chat_entry::Entry::CustomerMsg(msg_text)),
                                        };

                                        let sess = current_session.as_ref().unwrap();
                                        let mut s = sess.write().unwrap();
                                        s.history.push(entry.clone());
                                        // Broadcast to agents
                                        for tx in s.agent_txs.values() {
                                            let _ = tx.try_send(Some(entry.clone()));
                                        }
                                    }
                                    Some(pb::chat_customer_request::Req::HangUp(_)) => {
                                        if current_session.is_none() {
                                            let _ = out_tx.send(Err(Status::failed_precondition(
                                                "never called init, no chat session to hang up"
                                            ))).await;
                                            break;
                                        }

                                        close_session(&state, current_session.as_ref().unwrap());
                                        cleanup(&current_session, &cust);
                                        current_session = None;
                                        listener_rx = None;
                                    }
                                    None => {
                                        let _ = out_tx.send(Err(Status::invalid_argument(
                                            "unknown request type"
                                        ))).await;
                                        cleanup(&current_session, &cust);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    // Receive agent messages forwarded to customer
                    entry = async {
                        match listener_rx.as_mut() {
                            Some(rx) => rx.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        match entry {
                            Some(Some(chat_entry)) => {
                                // Forward agent messages to customer
                                if let Some(pb::chat_entry::Entry::AgentMsg(ref agent_msg)) = chat_entry.entry {
                                    let _ = out_tx.send(Ok(pb::ChatCustomerResponse {
                                        resp: Some(pb::chat_customer_response::Resp::Msg(agent_msg.clone())),
                                    })).await;
                                }
                            }
                            Some(None) | None => {
                                // Session ended or channel closed
                            }
                        }
                    }
                }
            }
        });

        let out_stream = ReceiverStream::new(out_rx);
        Ok(Response::new(Box::pin(out_stream)))
    }

    type ChatAgentStream =
        Pin<Box<dyn futures_core::Stream<Item = Result<pb::ChatAgentResponse, Status>> + Send>>;

    async fn chat_agent(
        &self,
        request: Request<Streaming<pb::ChatAgentRequest>>,
    ) -> Result<Response<Self::ChatAgentStream>, Status> {
        let agent = auth::get_agent(request.metadata())
            .ok_or_else(|| Status::unauthenticated("Unauthenticated"))?;

        let mut in_stream = request.into_inner();
        let (out_tx, out_rx) = mpsc::channel::<Result<pb::ChatAgentResponse, Status>>(32);
        let state = Arc::clone(&self.state);

        tokio::spawn(async move {
            let mut current_session: Option<Arc<RwLock<Session>>> = None;
            let mut listener_rx: Option<mpsc::Receiver<Option<pb::ChatEntry>>> = None;

            let cleanup = |sess: &Option<Arc<RwLock<Session>>>,
                           agent: &str,
                           state: &Arc<Mutex<ChatState>>| {
                if let Some(ref sess_lock) = sess {
                    let mut s = sess_lock.write().unwrap();
                    s.agent_txs.remove(agent);
                    if s.agent_txs.is_empty() && s.active {
                        let mut st = state.lock().unwrap();
                        st.awaiting_agent.push(s.id.clone());
                    }
                }
            };

            loop {
                // Check if session was concurrently closed
                if let Some(ref sess_lock) = current_session {
                    let active = sess_lock.read().unwrap().active;
                    if !active {
                        cleanup(&current_session, &agent, &state);
                        current_session = None;
                        listener_rx = None;
                    }
                }

                tokio::select! {
                    msg = in_stream.next() => {
                        match msg {
                            None => {
                                cleanup(&current_session, &agent, &state);
                                break;
                            }
                            Some(Err(_)) => {
                                cleanup(&current_session, &agent, &state);
                                break;
                            }
                            Some(Ok(req)) => {
                                match req.req {
                                    Some(pb::chat_agent_request::Req::Accept(accept)) => {
                                        if let Some(sess) = &current_session {
                                            let id = sess.read().unwrap().id.clone();
                                            let _ = out_tx.send(Err(Status::failed_precondition(
                                                format!("already called accept, currently in chat session {:?}", id)
                                            ))).await;
                                            cleanup(&current_session, &agent, &state);
                                            break;
                                        }

                                        match accept_session(&state, &agent, &accept.session_id) {
                                            Some((sess, rx)) => {
                                                let session_proto = sess.read().unwrap().copy_session_proto();
                                                let _ = out_tx.send(Ok(pb::ChatAgentResponse {
                                                    resp: Some(pb::chat_agent_response::Resp::AcceptedSession(session_proto)),
                                                })).await;
                                                current_session = Some(sess);
                                                listener_rx = Some(rx);
                                            }
                                            None => {
                                                let _ = out_tx.send(Err(Status::failed_precondition(
                                                    "no session to accept"
                                                ))).await;
                                                break;
                                            }
                                        }
                                    }
                                    Some(pb::chat_agent_request::Req::Msg(msg_text)) => {
                                        if current_session.is_none() {
                                            let _ = out_tx.send(Err(Status::failed_precondition(
                                                "never called accept, no chat session for message"
                                            ))).await;
                                            break;
                                        }

                                        let entry = pb::ChatEntry {
                                            date: Some(now()),
                                            entry: Some(pb::chat_entry::Entry::AgentMsg(pb::AgentMessage {
                                                agent_name: agent.clone(),
                                                msg: msg_text,
                                            })),
                                        };

                                        let sess = current_session.as_ref().unwrap();
                                        let session_inactive = {
                                            let mut s = sess.write().unwrap();
                                            if !s.active {
                                                true
                                            } else {
                                                s.history.push(entry.clone());
                                                // Send to customer
                                                if let Some(ref cust_tx) = s.customer_tx {
                                                    let _ = cust_tx.try_send(Some(entry.clone()));
                                                }
                                                // Send to other agents
                                                for (other_agent, tx) in &s.agent_txs {
                                                    if other_agent != &agent {
                                                        let _ = tx.try_send(Some(entry.clone()));
                                                    }
                                                }
                                                false
                                            }
                                        }; // guard dropped here
                                        if session_inactive {
                                            let id = sess.read().unwrap().id.clone();
                                            let _ = out_tx.send(Err(Status::failed_precondition(
                                                format!("customer hung up on chat session {}", id)
                                            ))).await;
                                            cleanup(&current_session, &agent, &state);
                                            break;
                                        }
                                    }
                                    Some(pb::chat_agent_request::Req::LeaveSession(_)) => {
                                        if current_session.is_none() {
                                            let _ = out_tx.send(Err(Status::failed_precondition(
                                                "never called init, no chat session to hang up"
                                            ))).await;
                                            break;
                                        }

                                        cleanup(&current_session, &agent, &state);
                                        current_session = None;
                                        listener_rx = None;
                                    }
                                    None => {
                                        let _ = out_tx.send(Err(Status::invalid_argument(
                                            "unknown request type"
                                        ))).await;
                                        cleanup(&current_session, &agent, &state);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    entry = async {
                        match listener_rx.as_mut() {
                            Some(rx) => rx.recv().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        match entry {
                            Some(None) => {
                                // Session ended by customer
                                let _ = out_tx.send(Ok(pb::ChatAgentResponse {
                                    resp: Some(pb::chat_agent_response::Resp::SessionEnded(0)),
                                })).await;
                            }
                            Some(Some(chat_entry)) => {
                                // Skip own messages
                                if let Some(pb::chat_entry::Entry::AgentMsg(ref agent_msg)) = chat_entry.entry {
                                    if agent_msg.agent_name == agent {
                                        continue;
                                    }
                                }
                                let _ = out_tx.send(Ok(pb::ChatAgentResponse {
                                    resp: Some(pb::chat_agent_response::Resp::Msg(chat_entry)),
                                })).await;
                            }
                            None => {
                                // Channel closed
                            }
                        }
                    }
                }
            }
        });

        let out_stream = ReceiverStream::new(out_rx);
        Ok(Response::new(Box::pin(out_stream)))
    }
}

fn new_session(
    state: &Arc<Mutex<ChatState>>,
    cust: &str,
) -> (Arc<RwLock<Session>>, mpsc::Receiver<Option<pb::ChatEntry>>) {
    let mut st = state.lock().unwrap();
    st.last_session += 1;
    let id = format!("{:06}", st.last_session);
    st.awaiting_agent.push(id.clone());

    let (tx, rx) = mpsc::channel(32);
    let sess = Arc::new(RwLock::new(Session {
        id: id.clone(),
        customer_name: cust.to_string(),
        history: Vec::new(),
        active: true,
        customer_tx: Some(tx),
        agent_txs: HashMap::new(),
    }));
    st.sessions.insert(id, Arc::clone(&sess));
    (sess, rx)
}

fn resume_session(
    state: &Arc<Mutex<ChatState>>,
    cust: &str,
    session_id: &str,
) -> Option<SessionResult> {
    let st = state.lock().unwrap();
    let sess = st.sessions.get(session_id)?;
    let s = sess.read().unwrap();
    if s.customer_name != cust {
        return None;
    }
    if !s.active {
        return None;
    }
    if s.customer_tx.is_some() {
        return None;
    }
    drop(s);

    let (tx, rx) = mpsc::channel(32);
    let mut s = sess.write().unwrap();
    s.customer_tx = Some(tx);
    drop(s);

    Some((Arc::clone(sess), rx))
}

fn close_session(state: &Arc<Mutex<ChatState>>, sess_lock: &Arc<RwLock<Session>>) {
    let mut s = sess_lock.write().unwrap();
    if !s.active {
        return;
    }
    s.active = false;

    // Notify agents that session ended
    for tx in s.agent_txs.values() {
        let _ = tx.try_send(None);
    }
    let session_id = s.id.clone();
    drop(s);

    let mut st = state.lock().unwrap();
    st.sessions.remove(&session_id);
    st.awaiting_agent.retain(|id| id != &session_id);
}

fn accept_session(
    state: &Arc<Mutex<ChatState>>,
    agent: &str,
    session_id: &str,
) -> Option<SessionResult> {
    let mut st = state.lock().unwrap();

    if st.awaiting_agent.is_empty() {
        return None;
    }

    let target_id = if session_id.is_empty() {
        st.awaiting_agent.remove(0)
    } else {
        let pos = st.awaiting_agent.iter().position(|id| id == session_id)?;
        st.awaiting_agent.remove(pos)
    };

    let sess = st.sessions.get(&target_id)?;
    let sess = Arc::clone(sess);
    drop(st);

    let (tx, rx) = mpsc::channel(32);
    let mut s = sess.write().unwrap();
    s.agent_txs.insert(agent.to_string(), tx);
    drop(s);

    Some((sess, rx))
}
