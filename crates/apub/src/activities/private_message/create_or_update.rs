use crate::{
  activities::{generate_activity_id, send_lemmy_activity, verify_activity, verify_person},
  fetcher::object_id::ObjectId,
  objects::{person::ApubPerson, private_message::ApubPrivateMessage},
  protocol::activities::{
    private_message::create_or_update::CreateOrUpdatePrivateMessage,
    CreateOrUpdateType,
  },
};
use lemmy_api_common::blocking;
use lemmy_apub_lib::{
  data::Data,
  traits::{ActivityHandler, ActorType, ApubObject},
  verify::verify_domains_match,
};
use lemmy_db_schema::{source::person::Person, traits::Crud};
use lemmy_utils::LemmyError;
use lemmy_websocket::{send::send_pm_ws_message, LemmyContext, UserOperationCrud};

impl CreateOrUpdatePrivateMessage {
  pub async fn send(
    private_message: &ApubPrivateMessage,
    actor: &ApubPerson,
    kind: CreateOrUpdateType,
    context: &LemmyContext,
  ) -> Result<(), LemmyError> {
    let recipient_id = private_message.recipient_id;
    let recipient: ApubPerson =
      blocking(context.pool(), move |conn| Person::read(conn, recipient_id))
        .await??
        .into();

    let id = generate_activity_id(
      kind.clone(),
      &context.settings().get_protocol_and_hostname(),
    )?;
    let create_or_update = CreateOrUpdatePrivateMessage {
      id: id.clone(),
      actor: ObjectId::new(actor.actor_id()),
      to: [ObjectId::new(recipient.actor_id())],
      object: private_message.to_apub(context).await?,
      kind,
      unparsed: Default::default(),
    };
    let inbox = vec![recipient.shared_inbox_or_inbox_url()];
    send_lemmy_activity(context, &create_or_update, &id, actor, inbox, true).await
  }
}
#[async_trait::async_trait(?Send)]
impl ActivityHandler for CreateOrUpdatePrivateMessage {
  type DataType = LemmyContext;
  async fn verify(
    &self,
    context: &Data<LemmyContext>,
    request_counter: &mut i32,
  ) -> Result<(), LemmyError> {
    verify_activity(self, &context.settings())?;
    verify_person(&self.actor, context, request_counter).await?;
    verify_domains_match(self.actor.inner(), self.object.id_unchecked())?;
    self.object.verify(context, request_counter).await?;
    Ok(())
  }

  async fn receive(
    self,
    context: &Data<LemmyContext>,
    request_counter: &mut i32,
  ) -> Result<(), LemmyError> {
    let private_message =
      ApubPrivateMessage::from_apub(&self.object, context, self.actor.inner(), request_counter)
        .await?;

    let notif_type = match self.kind {
      CreateOrUpdateType::Create => UserOperationCrud::CreatePrivateMessage,
      CreateOrUpdateType::Update => UserOperationCrud::EditPrivateMessage,
    };
    send_pm_ws_message(private_message.id, notif_type, None, context).await?;

    Ok(())
  }
}