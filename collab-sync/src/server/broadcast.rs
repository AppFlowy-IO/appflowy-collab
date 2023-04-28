use std::sync::Arc;

use collab::core::collab_awareness::MutexCollabAwareness;
use futures_util::{SinkExt, StreamExt};

use lib0::encoding::Write;
use tokio::select;
use tokio::sync::broadcast::error::SendError;
use tokio::sync::broadcast::{channel, Sender};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use y_sync::awareness;
use y_sync::awareness::{Awareness, AwarenessUpdate};
use y_sync::sync::{Message, MSG_SYNC, MSG_SYNC_UPDATE};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::{Encode, Encoder, EncoderV1};
use yrs::UpdateSubscription;

use crate::error::SyncError;
use crate::message::{CollabMessage, CollabServerMessage};
use crate::protocol::{handle_msg, CollabSyncProtocol};

/// A broadcast group can be used to propagate updates produced by yrs [yrs::Doc] and [Awareness]
/// to subscribes.
pub struct BroadcastGroup {
  object_id: String,
  #[allow(dead_code)]
  awareness_sub: awareness::UpdateSubscription,
  #[allow(dead_code)]
  doc_sub: UpdateSubscription,
  awareness: MutexCollabAwareness,
  sender: Sender<CollabMessage>,
}

impl BroadcastGroup {
  /// Creates a new [BroadcastGroup] over a provided `awareness` instance. All changes triggered
  /// by this awareness structure or its underlying document will be propagated to all subscribers
  /// which have been registered via [BroadcastGroup::subscribe] method.
  ///
  /// The overflow of the incoming events that needs to be propagates will be buffered up to a
  /// provided `buffer_capacity` size.
  pub async fn new(
    object_id: &str,
    awareness: MutexCollabAwareness,
    buffer_capacity: usize,
  ) -> Self {
    let object_id = object_id.to_owned();
    let (sender, _) = channel(buffer_capacity);
    let (doc_sub, awareness_sub) = {
      let mut awareness = awareness.lock();

      // Observer the document's update and broadcast it to all subscribers.
      let cloned_oid = object_id.clone();
      let sink = sender.clone();
      let doc_sub = awareness
        .doc_mut()
        .observe_update_v1(move |_txn, event| {
          let payload = gen_update_message(&event.update);
          let msg = CollabServerMessage::new(cloned_oid.clone(), payload);
          if let Err(_e) = sink.send(msg.into()) {
            tracing::trace!("Broadcast group is closed");
          }
        })
        .unwrap();

      // Observer the awareness's update and broadcast it to all subscribers.
      let sink = sender.clone();
      let cloned_oid = object_id.clone();
      let awareness_sub = awareness.on_update(move |awareness, event| {
        if let Ok(u) = gen_awareness_update_message(awareness, event) {
          let payload = Message::Awareness(u).encode_v1();
          let msg = CollabServerMessage::new(cloned_oid.clone(), payload);
          if let Err(_e) = sink.send(msg.into()) {
            tracing::trace!("Broadcast group is closed");
          }
        }
      });
      (doc_sub, awareness_sub)
    };
    BroadcastGroup {
      object_id,
      awareness,
      sender,
      awareness_sub,
      doc_sub,
    }
  }

  /// Returns a reference to an underlying [CollabAwareness] instance.
  pub fn awareness(&self) -> &MutexCollabAwareness {
    &self.awareness
  }

  /// Broadcasts user message to all active subscribers. Returns error if message could not have
  /// been broadcast.
  pub fn broadcast(&self, msg: CollabServerMessage) -> Result<(), SendError<CollabMessage>> {
    self.sender.send(msg.into())?;
    Ok(())
  }

  /// Subscribes a new connection - represented by `sink`/`stream` pair implementing a futures
  /// Sink and Stream protocols - to a current broadcast group.
  ///
  /// Returns a subscription structure, which can be dropped in order to unsubscribe or awaited
  /// via [Subscription::completed] method in order to complete of its own volition (due to
  /// an internal connection error or closed connection).
  pub fn subscribe<Sink, Stream, E>(
    &self,
    sink: Arc<Mutex<Sink>>,
    mut stream: Stream,
  ) -> Subscription
  where
    Sink: SinkExt<CollabMessage> + Send + Sync + Unpin + 'static,
    Stream: StreamExt<Item = Result<CollabMessage, E>> + Send + Sync + Unpin + 'static,
    <Sink as futures_util::Sink<CollabMessage>>::Error: std::error::Error + Send + Sync,
    E: std::error::Error + Send + Sync + 'static,
  {
    tracing::trace!("New client connected");
    // Receive a new message from client and forwarding the message to the other clients
    let sink_task = {
      let sink = sink.clone();
      let mut receiver = self.sender.subscribe();
      tokio::spawn(async move {
        while let Ok(msg) = receiver.recv().await {
          tracing::trace!("Broadcast client message: {}", msg);
          let mut sink = sink.lock().await;
          if let Err(e) = sink.send(msg).await {
            tracing::error!("Broadcast client message failed: {:?}", e);
            return Err(SyncError::Internal(Box::new(e)));
          }
        }
        Ok(())
      })
    };

    // Receive the message from the client and reply with the response
    let stream_task = {
      let awareness = self.awareness().clone();
      let object_id = self.object_id.clone();
      tokio::spawn(async move {
        while let Some(res) = stream.next().await {
          let msg = res.map_err(|e| SyncError::Internal(Box::new(e)))?;
          tracing::trace!("Client message: {}", msg);
          let mut decoder = DecoderV1::from(msg.payload().as_ref());
          while let Ok(msg) = Message::decode(&mut decoder) {
            let reply = handle_msg(&CollabSyncProtocol, &awareness, msg).await?;
            match reply {
              None => {},
              Some(reply) => {
                let mut sink = sink.lock().await;
                let payload = reply.encode_v1();
                let msg = CollabServerMessage::new(object_id.clone(), payload);
                sink
                  .send(msg.into())
                  .await
                  .map_err(|e| SyncError::Internal(Box::new(e)))?;
              },
            }
          }
        }
        Ok(())
      })
    };

    Subscription {
      sink_task,
      stream_task,
    }
  }
}

/// A subscription structure returned from [BroadcastGroup::subscribe], which represents a
/// subscribed connection. It can be dropped in order to unsubscribe or awaited via
/// [Subscription::completed] method in order to complete of its own volition (due to an internal
/// connection error or closed connection).
#[derive(Debug)]
pub struct Subscription {
  sink_task: JoinHandle<Result<(), SyncError>>,
  stream_task: JoinHandle<Result<(), SyncError>>,
}

impl Subscription {
  /// Consumes current subscription, waiting for it to complete. If an underlying connection was
  /// closed because of failure, an error which caused it to happen will be returned.
  ///
  /// This method doesn't invoke close procedure. If you need that, drop current subscription instead.
  pub async fn completed(self) -> Result<(), SyncError> {
    let res = select! {
        r1 = self.sink_task => r1?,
        r2 = self.stream_task => r2?,
    };
    res
  }
}

fn gen_update_message(update: &[u8]) -> Vec<u8> {
  let mut encoder = EncoderV1::new();
  encoder.write_var(MSG_SYNC);
  encoder.write_var(MSG_SYNC_UPDATE);
  encoder.write_buf(update);
  encoder.to_vec()
}

fn gen_awareness_update_message(
  awareness: &Awareness,
  event: &awareness::Event,
) -> Result<AwarenessUpdate, SyncError> {
  let added = event.added();
  let updated = event.updated();
  let removed = event.removed();
  let mut changed = Vec::with_capacity(added.len() + updated.len() + removed.len());
  changed.extend_from_slice(added);
  changed.extend_from_slice(updated);
  changed.extend_from_slice(removed);
  let update = awareness.update_with_clients(changed)?;
  Ok(update)
}