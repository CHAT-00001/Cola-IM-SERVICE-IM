// server/src/hub.rs -- Transport-independent connection and room registry
// 2026/10/7 14:03 Created.

////////
///
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::chat::port::{ChatError, DeviceConnectionRegistry};
use crate::models::message::MessageBody;

type ConnectionId = String;

struct HubState {
    senders: HashMap<ConnectionId, mpsc::Sender<MessageBody>>,
    connection_identity: HashMap<ConnectionId, (u64, String)>,
    account_devices: HashMap<u64, HashMap<String, HashSet<ConnectionId>>>,
    rooms: HashMap<String, HashSet<ConnectionId>>,
    connection_rooms: HashMap<ConnectionId, HashSet<String>>,
}

////////

#[derive(Clone)]
pub struct ChatHub {
    state: Arc<Mutex<HubState>>,
    queue_capacity: usize,
}

////////

/// # [HANDLER] - ChatHub
/// * `DESC`: `传输无关的连接注册表和房间成员管理
impl ChatHub {
    /// # [HANDLER] - new
    /// * `DESC`: `创建带有界发送队列的传输无关连接注册表`
    pub fn new(queue_capacity: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(HubState {
                senders: HashMap::new(),
                connection_identity: HashMap::new(),
                account_devices: HashMap::new(),
                rooms: HashMap::new(),
                connection_rooms: HashMap::new(),
            })),
            queue_capacity: queue_capacity.max(1),
        }
    }

    /// # [HANDLER] - register
    /// * `DESC`: `注册连接并返回该连接的有界出站消息接收端`
    pub fn register(&self, connection_id: ConnectionId) -> mpsc::Receiver<MessageBody> {
        let (sender, receiver) = mpsc::channel(self.queue_capacity);
        let mut state = self.state.lock().expect("chat hub mutex poisoned");
        if let Some(rooms) = state.connection_rooms.remove(&connection_id) {
            for room_id in rooms {
                if let Some(members) = state.rooms.get_mut(&room_id) {
                    members.remove(&connection_id);
                    if members.is_empty() {
                        state.rooms.remove(&room_id);
                    }
                }
            }
        }
        state.senders.insert(connection_id.clone(), sender);
        state.connection_rooms.insert(connection_id, HashSet::new());
        receiver
    }

    /// # [HANDLER] - unregister
    /// * `DESC`: `移除连接以及它在所有房间中的成员关系`
    pub fn unregister(&self, connection_id: &str) {
        let mut state = self.state.lock().expect("chat hub mutex poisoned");
        state.senders.remove(connection_id);
        if let Some((account_id, device_id)) = state.connection_identity.remove(connection_id) {
            if let Some(devices) = state.account_devices.get_mut(&account_id) {
                if let Some(connections) = devices.get_mut(&device_id) {
                    connections.remove(connection_id);
                    if connections.is_empty() {
                        devices.remove(&device_id);
                    }
                }
                if devices.is_empty() {
                    state.account_devices.remove(&account_id);
                }
            }
        }
        if let Some(rooms) = state.connection_rooms.remove(connection_id) {
            for room_id in rooms {
                if let Some(members) = state.rooms.get_mut(&room_id) {
                    members.remove(connection_id);
                    if members.is_empty() {
                        state.rooms.remove(&room_id);
                    }
                }
            }
        }
    }

    /// # [HANDLER] - register_authenticated_device
    /// * `DESC`: `将已认证账号设备绑定到长连接并支持同设备多连接`
    pub fn register_authenticated_device(
        &self,
        connection_id: &str,
        account_id: u64,
        device_id: &str,
    ) -> bool {
        if device_id.is_empty() || device_id.len() > 128 {
            return false;
        }
        let mut state = self.state.lock().expect("chat hub mutex poisoned");
        if !state.senders.contains_key(connection_id) {
            return false;
        }
        if let Some((old_account_id, old_device_id)) =
            state.connection_identity.remove(connection_id)
        {
            if let Some(devices) = state.account_devices.get_mut(&old_account_id) {
                if let Some(connections) = devices.get_mut(&old_device_id) {
                    connections.remove(connection_id);
                    if connections.is_empty() {
                        devices.remove(&old_device_id);
                    }
                }
                if devices.is_empty() {
                    state.account_devices.remove(&old_account_id);
                }
            }
        }
        state
            .connection_identity
            .insert(connection_id.to_owned(), (account_id, device_id.to_owned()));
        state
            .account_devices
            .entry(account_id)
            .or_default()
            .entry(device_id.to_owned())
            .or_default()
            .insert(connection_id.to_owned());
        true
    }

    /// # [HANDLER] - matches_authenticated_session
    /// * `DESC`: `确认会话连接已绑定到提供的可信账号与设备身份`
    pub fn matches_authenticated_session(
        &self,
        connection_id: &str,
        account_id: u64,
        device_id: &str,
    ) -> bool {
        self.state
            .lock()
            .expect("chat hub mutex poisoned")
            .connection_identity
            .get(connection_id)
            .is_some_and(|identity| identity == &(account_id, device_id.to_owned()))
    }

    /// # [HANDLER] - enqueue_to_connection
    /// * `DESC`: `将单条消息投递到指定长连接的有界出站队列`
    pub fn enqueue_to_connection(
        &self,
        connection_id: &str,
        message: MessageBody,
    ) -> Result<(), ChatError> {
        let sender = self
            .state
            .lock()
            .expect("chat hub mutex poisoned")
            .senders
            .get(connection_id)
            .cloned()
            .ok_or(ChatError::MessageNotFound)?;
        sender.try_send(message).map_err(|_| {
            self.unregister(connection_id);
            ChatError::Persistence("connection outbound queue is full or closed".to_owned())
        })
    }

    ////////

    /// # [HANDLER] - join_room
    /// * `DESC`: `将连接加入房间并限制单连接可加入的房间数`
    pub fn join_room(&self, connection_id: &str, room_id: &str, max_rooms: usize) -> bool {
        if room_id.is_empty() {
            return false;
        }

        let mut state = self.state.lock().expect("chat hub mutex poisoned");
        if !state.senders.contains_key(connection_id) {
            return false;
        }

        let Some(connection_rooms) = state.connection_rooms.get_mut(connection_id) else {
            return false;
        };
        if connection_rooms.contains(room_id) {
            return true;
        }
        if connection_rooms.len() >= max_rooms {
            return false;
        }

        connection_rooms.insert(room_id.to_owned());
        state
            .rooms
            .entry(room_id.to_owned())
            .or_default()
            .insert(connection_id.to_owned());
        true
    }

    ////////

    /// # [HANDLER] - broadcast
    /// * `DESC`: `向房间成员投递消息并移除出站队列满或已关闭的慢连接`
    pub fn broadcast(&self, room_id: &str, message: MessageBody) -> usize {
        let (recipients, members) = {
            let state = self.state.lock().expect("chat hub mutex poisoned");
            let Some(members) = state.rooms.get(room_id) else {
                return 0;
            };
            let recipients = members
                .iter()
                .filter_map(|connection_id| {
                    state
                        .senders
                        .get(connection_id)
                        .map(|sender| (connection_id.clone(), sender.clone()))
                })
                .collect::<Vec<_>>();
            (recipients, members.clone())
        };

        let mut delivered = 0;
        let mut slow_connections = HashSet::new();
        for (connection_id, sender) in recipients {
            match sender.try_send(message.clone()) {
                Ok(()) => delivered += 1,
                Err(mpsc::error::TrySendError::Full(_))
                | Err(mpsc::error::TrySendError::Closed(_)) => {
                    slow_connections.insert(connection_id);
                }
            }
        }

        for connection_id in slow_connections {
            self.unregister(&connection_id);
        }

        delivered.min(members.len())
    }

    #[cfg(test)]
    fn room_member_count(&self, room_id: &str) -> usize {
        self.state
            .lock()
            .expect("chat hub mutex poisoned")
            .rooms
            .get(room_id)
            .map_or(0, HashSet::len)
    }
}

#[async_trait::async_trait]
impl DeviceConnectionRegistry for ChatHub {
    async fn online_device_ids(&self, account_id: u64) -> Vec<String> {
        self.state
            .lock()
            .expect("chat hub mutex poisoned")
            .account_devices
            .get(&account_id)
            .map(|devices| {
                devices
                    .iter()
                    .filter(|(_, connections)| !connections.is_empty())
                    .map(|(device_id, _)| device_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn enqueue_to_device(
        &self,
        account_id: u64,
        device_id: &str,
        message: &MessageBody,
    ) -> Result<(), ChatError> {
        let senders = {
            let state = self.state.lock().expect("chat hub mutex poisoned");
            let Some(connections) = state
                .account_devices
                .get(&account_id)
                .and_then(|devices| devices.get(device_id))
            else {
                return Ok(());
            };
            connections
                .iter()
                .filter_map(|connection_id| {
                    state
                        .senders
                        .get(connection_id)
                        .map(|sender| (connection_id.clone(), sender.clone()))
                })
                .collect::<Vec<_>>()
        };

        let mut slow_connections = Vec::new();
        for (connection_id, sender) in senders {
            if sender.try_send(message.clone()).is_err() {
                slow_connections.push(connection_id);
            }
        }
        for connection_id in slow_connections {
            self.unregister(&connection_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ChatHub;
    use crate::chat::port::DeviceConnectionRegistry;
    use crate::models::message::MessageBody;

    /// # [HANDLER] - hub_broadcasts_between_registered_connections
    /// * `DESC`: `验证注册连接可通过统一房间注册表收到广播`
    #[tokio::test]
    async fn hub_broadcasts_between_registered_connections() {
        let hub = ChatHub::new(2);
        let mut first = hub.register("first".to_owned());
        let mut second = hub.register("second".to_owned());
        assert!(hub.join_room("first", "room", 20));
        assert!(hub.join_room("second", "room", 20));

        assert_eq!(hub.broadcast("room", test_message()), 2);
        assert_eq!(first.recv().await.unwrap().room_id, "room");
        assert_eq!(second.recv().await.unwrap().room_id, "room");
    }

    /// # [HANDLER] - hub_enforces_room_limit_and_cleans_up
    /// * `DESC`: `验证房间上限和连接注销后的成员清理`
    #[test]
    fn hub_enforces_room_limit_and_cleans_up() {
        let hub = ChatHub::new(1);
        let _receiver = hub.register("client".to_owned());
        assert!(hub.join_room("client", "one", 1));
        assert!(!hub.join_room("client", "two", 1));
        assert_eq!(hub.room_member_count("one"), 1);

        hub.unregister("client");
        assert_eq!(hub.room_member_count("one"), 0);
    }

    /// # [HANDLER] - hub_removes_slow_connections
    /// * `DESC`: `验证发送队列满时注销慢连接以限制背压`
    #[test]
    fn hub_removes_slow_connections() {
        let hub = ChatHub::new(1);
        let _receiver = hub.register("slow".to_owned());
        assert!(hub.join_room("slow", "room", 20));

        assert_eq!(hub.broadcast("room", test_message()), 1);
        assert_eq!(hub.broadcast("room", test_message()), 0);
        assert_eq!(hub.room_member_count("room"), 0);
    }

    /// # [HANDLER] - hub_tracks_multiple_devices_per_account
    /// * `DESC`: `验证一个账号可同时连接多个设备并投递到指定设备`
    #[tokio::test]
    async fn hub_tracks_multiple_devices_per_account() {
        let hub = ChatHub::new(2);
        let mut phone = hub.register("phone-connection".to_owned());
        let mut tablet = hub.register("tablet-connection".to_owned());
        assert!(hub.register_authenticated_device("phone-connection", 7, "phone"));
        assert!(hub.register_authenticated_device("tablet-connection", 7, "tablet"));

        let mut devices = hub.online_device_ids(7).await;
        devices.sort();
        assert_eq!(devices, vec!["phone", "tablet"]);
        hub.enqueue_to_device(7, "tablet", &test_message())
            .await
            .unwrap();
        assert!(phone.try_recv().is_err());
        assert_eq!(tablet.recv().await.unwrap().room_id, "room");

        hub.unregister("tablet-connection");
        assert_eq!(hub.online_device_ids(7).await, vec!["phone"]);
    }

    /// # [HANDLER] - authenticated_binding_cannot_be_reassigned_without_cleanup
    /// * `DESC`: `验证认证身份变更会清理旧账号设备映射并拒绝身份不匹配`
    #[test]
    fn authenticated_binding_cannot_be_reassigned_without_cleanup() {
        let hub = ChatHub::new(2);
        let _receiver = hub.register("connection".to_owned());
        assert!(hub.register_authenticated_device("connection", 7, "phone"));
        assert!(hub.matches_authenticated_session("connection", 7, "phone"));
        assert!(!hub.matches_authenticated_session("connection", 8, "phone"));

        assert!(hub.register_authenticated_device("connection", 8, "tablet"));
        assert!(!hub.matches_authenticated_session("connection", 7, "phone"));
        assert!(hub.matches_authenticated_session("connection", 8, "tablet"));
    }

    fn test_message() -> MessageBody {
        use crate::models::message::{Event, MsgType, Payload, Status};
        use chrono::Utc;

        MessageBody {
            status: Status::Sending,
            client_id: "client".to_owned(),
            send_at: Utc::now(),
            server_sync_at: None,
            send_id: "message".to_owned(),
            sync_id: String::new(),
            msg_type: MsgType::Text,
            event: Event::SendMsg,
            room_id: "room".to_owned(),
            uid: 7,
            sender: None,
            payload: Payload::Content {
                text: "hello".to_owned(),
            },
            channel: None,
            recipient_uid: None,
            ack_message_id: None,
            device_id: None,
            ack_stage: None,
        }
    }
}

//////// END
