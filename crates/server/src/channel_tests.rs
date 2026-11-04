//! Channels that several people run, end to end over HTTP: making one, inviting a person, who
//! accepts with the address Google vouched for, roles that open what they say and no more, acting
//! as the channel, keeping an owner, leaving, and deleting it.

use serde_json::{Value, json};
use youcraft_api::ChannelId;

use crate::http::tests::{answer, ready_video, sign_in, site_where};

/// The status of `id` on `input` for `cookie`.
async fn status(
    routes: &axum::Router,
    id: &str,
    input: Value,
    cookie: &str,
) -> axum::http::StatusCode {
    answer(routes, id, input, cookie).await.0
}

#[tokio::test]
async fn a_channel_is_run_by_whom_its_owners_invite_as_what_they_say() {
    use axum::http::StatusCode;

    let (site, routes) = site_where(youcraft_api::Grant::Creator);
    let ada = sign_in(&routes, "ada").await;
    let bob = sign_in(&routes, "bob").await;
    let (_, ada_me) = answer(&routes, "account.me", json!({}), &ada).await;
    let (_, bob_me) = answer(&routes, "account.me", json!({}), &bob).await;
    let personal = ada_me["channel"]["id"].clone();

    let (made, lab) = answer(
        &routes,
        "channel.create",
        json!({ "name": "Ada's lab" }),
        &ada,
    )
    .await;
    assert_eq!(made, StatusCode::OK, "{lab}");
    let channel = lab["id"].clone();
    let (_, ada_me) = answer(&routes, "account.me", json!({}), &ada).await;
    assert_eq!(
        ada_me["channel"]["id"], channel,
        "she acts as what she made"
    );
    assert_eq!(ada_me["channels"].as_array().unwrap().len(), 2);

    let invite = json!({ "channel": channel, "email": "BOB@example.com", "role": "editor" });
    assert_eq!(
        status(&routes, "member.invite", invite.clone(), &bob).await,
        StatusCode::FORBIDDEN,
        "only its owners invite"
    );
    assert_eq!(
        status(&routes, "member.invite", invite, &ada).await,
        StatusCode::OK
    );
    let (_, bob_me_now) = answer(&routes, "account.me", json!({}), &bob).await;
    assert_eq!(bob_me_now["invitations"], 1);
    let (_, offers) = answer(&routes, "member.offers", json!({}), &bob).await;
    assert_eq!(offers[0]["channel"]["id"], channel);
    let which = json!({ "channel": channel });
    assert_eq!(
        status(&routes, "member.accept", which.clone(), &ada).await,
        StatusCode::NOT_FOUND,
        "not her address"
    );
    let (joined, membership) = answer(&routes, "member.accept", which.clone(), &bob).await;
    assert_eq!(joined, StatusCode::OK, "{membership}");
    assert_eq!(membership["role"], "editor");

    // An editor works in the studio, but does not speak as the channel.
    assert_eq!(
        status(&routes, "account.act_as", which.clone(), &bob).await,
        StatusCode::FORBIDDEN
    );
    let upload = json!({ "channel": channel, "title": "Lab notes", "size": 5 });
    assert_eq!(
        status(&routes, "upload.create", upload, &bob).await,
        StatusCode::OK
    );
    let shelf = json!({ "channel": channel });
    let (_, studio) = answer(&routes, "studio.videos", shelf.clone(), &bob).await;
    assert_eq!(studio["videos"].as_array().unwrap().len(), 1, "{studio}");
    let change = json!({ "channel": channel, "name": "Bob's lab" });
    assert_eq!(
        status(&routes, "channel.update", change, &bob).await,
        StatusCode::FORBIDDEN,
        "how it shows itself is its owners' to say"
    );
    assert_eq!(
        status(&routes, "member.list", which.clone(), &bob).await,
        StatusCode::FORBIDDEN
    );

    // A manager speaks as it.
    let promote = json!({ "channel": channel, "account": bob_me["account"], "role": "manager" });
    assert_eq!(
        status(&routes, "member.set_role", promote, &ada).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&routes, "account.act_as", which.clone(), &bob).await,
        StatusCode::OK
    );
    let video = ready_video(
        &site,
        serde_json::from_value::<ChannelId>(personal.clone()).unwrap(),
    );
    let (_, comment) = answer(
        &routes,
        "comment.create",
        json!({ "video": video, "body": "From the lab." }),
        &bob,
    )
    .await;
    assert_eq!(comment["author"]["id"], channel, "{comment}");
    let (_, seen) = answer(&routes, "comment.list", json!({ "video": video }), &ada).await;
    assert_eq!(
        seen["comments"][0]["editable"], true,
        "Ada speaks as the lab too"
    );

    // The channel keeps an owner.
    let ada_seat = json!({ "channel": channel, "account": ada_me["account"] });
    assert_eq!(
        status(&routes, "member.remove", ada_seat.clone(), &ada).await,
        StatusCode::CONFLICT
    );
    let crown = json!({ "channel": channel, "account": bob_me["account"], "role": "owner" });
    assert_eq!(
        status(&routes, "member.set_role", crown, &ada).await,
        StatusCode::OK
    );
    assert_eq!(
        status(&routes, "member.remove", ada_seat, &ada).await,
        StatusCode::OK
    );
    let (_, ada_me) = answer(&routes, "account.me", json!({}), &ada).await;
    assert_eq!(
        ada_me["channel"]["id"], personal,
        "she acts as her own channel again"
    );
    assert_eq!(
        status(&routes, "channel.delete", which.clone(), &ada).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&routes, "channel.delete", which, &bob).await,
        StatusCode::OK
    );
    let (_, bob_me) = answer(&routes, "account.me", json!({}), &bob).await;
    assert_eq!(bob_me["channels"].as_array().unwrap().len(), 1);
    let (_, gone) = answer(&routes, "comment.list", json!({ "video": video }), &ada).await;
    assert_eq!(gone["comments"], json!([]), "its comments went with it");
}
