use crate::{
  activities::{
    generate_activity_id,
    send_lemmy_activity,
    verify_activity,
    verify_person,
    verify_person_in_community,
  },
  fetcher::object_id::ObjectId,
  objects::{community::ApubCommunity, person::ApubPerson},
  protocol::activities::following::{accept::AcceptFollowCommunity, follow::FollowCommunity},
};
use activitystreams::activity::kind::FollowType;
use lemmy_api_common::blocking;
use lemmy_apub_lib::{
  data::Data,
  traits::{ActivityHandler, ActorType},
  verify::verify_urls_match,
};
use lemmy_db_schema::{
  source::community::{CommunityFollower, CommunityFollowerForm},
  traits::Followable,
};
use lemmy_utils::LemmyError;
use lemmy_websocket::LemmyContext;

impl FollowCommunity {
  pub(in crate::activities::following) fn new(
    actor: &ApubPerson,
    community: &ApubCommunity,
    context: &LemmyContext,
  ) -> Result<FollowCommunity, LemmyError> {
    Ok(FollowCommunity {
      actor: ObjectId::new(actor.actor_id()),
      to: [ObjectId::new(community.actor_id())],
      object: ObjectId::new(community.actor_id()),
      kind: FollowType::Follow,
      id: generate_activity_id(
        FollowType::Follow,
        &context.settings().get_protocol_and_hostname(),
      )?,
      unparsed: Default::default(),
    })
  }
  pub async fn send(
    actor: &ApubPerson,
    community: &ApubCommunity,
    context: &LemmyContext,
  ) -> Result<(), LemmyError> {
    let community_follower_form = CommunityFollowerForm {
      community_id: community.id,
      person_id: actor.id,
      pending: true,
    };
    blocking(context.pool(), move |conn| {
      CommunityFollower::follow(conn, &community_follower_form).ok()
    })
    .await?;

    let follow = FollowCommunity::new(actor, community, context)?;
    let inbox = vec![community.inbox_url.clone().into()];
    send_lemmy_activity(context, &follow, &follow.id, actor, inbox, true).await
  }
}

#[async_trait::async_trait(?Send)]
impl ActivityHandler for FollowCommunity {
  type DataType = LemmyContext;
  async fn verify(
    &self,
    context: &Data<LemmyContext>,
    request_counter: &mut i32,
  ) -> Result<(), LemmyError> {
    verify_activity(self, &context.settings())?;
    verify_urls_match(self.to[0].inner(), self.object.inner())?;
    verify_person(&self.actor, context, request_counter).await?;
    let community = self.to[0].dereference(context, request_counter).await?;
    verify_person_in_community(&self.actor, &community, context, request_counter).await?;
    Ok(())
  }

  async fn receive(
    self,
    context: &Data<LemmyContext>,
    request_counter: &mut i32,
  ) -> Result<(), LemmyError> {
    let actor = self.actor.dereference(context, request_counter).await?;
    let community = self.to[0].dereference(context, request_counter).await?;
    let community_follower_form = CommunityFollowerForm {
      community_id: community.id,
      person_id: actor.id,
      pending: false,
    };

    // This will fail if they're already a follower, but ignore the error.
    blocking(context.pool(), move |conn| {
      CommunityFollower::follow(conn, &community_follower_form).ok()
    })
    .await?;

    AcceptFollowCommunity::send(self, context, request_counter).await
  }
}