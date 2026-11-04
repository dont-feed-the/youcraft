//! The comments under a watched video, as on YouTube: their count and order, a box to write
//! one, then the comments, each with its likes, a way to reply, its replies one level deep, and
//! a menu to edit, delete, or keep its author from commenting on the creator's videos.
//!
//! Comments load in the browser once the page is shown, page after page, as YouTube's do.

use leptos::prelude::*;
use youcraft_api::commands::channel::{self, Barring};
use youcraft_api::commands::comment::{
    self, BODY_LIMIT, Comment, Comments, Edit, Liking, NewComment, OfComment, OfVideo, Order, Which,
};
use youcraft_api::{ApiError, CommentId, VideoId, routes};

use crate::act::act;
use crate::browser;
use crate::call::call;
use crate::components::avatar::{Face, Size};
use crate::components::more::ShowMore;
use crate::icons::{Glyph, Icon};
use crate::lang::{Language, RenderTime, compact, count, tr};

/// Add `page` to `items`, and keep where the next one starts.
fn extend(items: RwSignal<Vec<Comment>>, next: RwSignal<Option<String>>, page: Comments) {
    items.update(|items| items.extend(page.comments));
    next.set(page.next);
}

/// The section, for `video` with `total` comments; `owned` when the reader made the video.
#[component]
pub fn CommentSection(video: VideoId, total: u64, enabled: bool, owned: bool) -> impl IntoView {
    if !enabled {
        return view! {
            <section class="comments"><p class="comments-off">{tr("Comments are turned off.")}</p></section>
        }
        .into_any();
    }
    let order = RwSignal::new(Order::Top);
    let items = RwSignal::new(Vec::<Comment>::new());
    let next = RwSignal::new(None::<String>);
    let total = RwSignal::new(total);
    Effect::new(move |_| {
        let order = order.get();
        leptos::task::spawn_local(async move {
            let first = OfVideo {
                video,
                order,
                after: None,
            };
            if let Ok(page) = call::<comment::List>(first).await {
                items.set(Vec::new());
                extend(items, next, page);
            }
        });
    });
    let more = Callback::new(move |()| {
        let after = next.get_untracked();
        let order = order.get_untracked();
        act::<comment::List>(
            OfVideo {
                video,
                order,
                after,
            },
            move |page| {
                if let Ok(page) = page {
                    extend(items, next, page);
                }
            },
        );
    });
    let posted = Callback::new(move |comment: Comment| {
        items.update(|items| items.insert(0, comment));
        total.update(|n| *n = n.saturating_add(1));
    });
    let removed = Callback::new(move |id: CommentId| {
        items.update(|items| items.retain(|c| c.id != id));
        total.update(|n| *n = n.saturating_sub(1));
    });
    view! {
        <section class="comments">
            <div class="comments-heading">
                <h2>{move || count(total.get(), "{n} comment", "{n} comments")}</h2>
                <SortMenu order/>
            </div>
            <Composer video parent=None posted cancel=None/>
            <For each=move || items.get() key=|comment| comment.id let:comment>
                <CommentView comment owned removed thread=None/>
            </For>
            <ShowMore next more/>
        </section>
    }
    .into_any()
}

/// The menu that orders the comments.
#[component]
fn SortMenu(order: RwSignal<Order>) -> impl IntoView {
    let open = RwSignal::new(false);
    let entry = move |which: Order, label: &'static str| {
        let ticked = move || order.get() == which;
        view! {
            <button class="menu-entry" role="menuitemradio" type="button"
                aria-checked=move || if ticked() { "true" } else { "false" }
                on:click=move |_| { order.set(which); open.set(false); }>
                <span class="menu-tick" class:ticked=ticked><Glyph icon=Icon::Check/></span>
                <span>{label}</span>
            </button>
        }
    };
    view! {
        <div class="popup-anchor">
            <button class="pill quiet" type="button" aria-haspopup="menu"
                aria-expanded=move || if open.get() { "true" } else { "false" }
                on:click=move |_| open.update(|open| *open = !*open)>
                <Glyph icon=Icon::Sort/><span>{tr("Sort by")}</span>
            </button>
            <Show when=move || open.get()>
                <div class="menu-backdrop" on:click=move |_| open.set(false)></div>
                <div class="popup" role="menu">
                    {entry(Order::Top, tr("Top comments"))}
                    {entry(Order::Newest, tr("Newest first"))}
                </div>
            </Show>
        </div>
    }
}

/// The box to write a comment or a reply; its buttons show once the person starts writing.
#[component]
fn Composer(
    video: VideoId,
    parent: Option<CommentId>,
    posted: Callback<Comment>,
    cancel: Option<Callback<()>>,
) -> AnyView {
    let body = RwSignal::new(String::new());
    let active = RwSignal::new(cancel.is_some());
    let sending = RwSignal::new(false);
    let send = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let text = body.get_untracked();
        if text.trim().is_empty() || sending.get_untracked() {
            return;
        }
        sending.set(true);
        act::<comment::Create>(
            NewComment {
                video,
                parent,
                body: text,
            },
            move |answer| {
                sending.set(false);
                if let Ok(comment) = answer {
                    body.set(String::new());
                    active.set(cancel.is_some());
                    posted.run(comment);
                }
            },
        );
    };
    let stop = move |_| {
        body.set(String::new());
        active.set(false);
        if let Some(cancel) = cancel {
            cancel.run(());
        }
    };
    let placeholder = if parent.is_some() {
        tr("Add a reply…")
    } else {
        tr("Add a comment…")
    };
    view! {
        <form class="composer" class:reply=parent.is_some() on:submit=send>
            <span class="avatar composer-avatar"><Glyph icon=Icon::Account/></span>
            <div class="composer-main">
                <textarea class="composer-input" rows="1"
                    placeholder=placeholder aria-label=placeholder prop:value=move || body.get()
                    on:focus=move |_| active.set(true)
                    on:input=move |event| body.set(browser::clipped(&event, BODY_LIMIT))></textarea>
                <Show when=move || active.get()>
                    <div class="composer-buttons">
                        <button class="pill quiet" type="button" on:click=stop>{tr("Cancel")}</button>
                        <button class="pill primary" type="submit"
                            disabled=move || body.get().trim().is_empty() || sending.get()>
                            {if parent.is_some() { tr("Reply") } else { tr("Comment") }}
                        </button>
                    </div>
                </Show>
            </div>
        </form>
    }
    .into_any()
}

/// One comment, with its replies if it has any; a reply's `thread` takes the replies written to
/// it, which join its comment's.
#[component]
fn CommentView(
    comment: Comment,
    owned: bool,
    removed: Callback<CommentId>,
    thread: Option<Callback<Comment>>,
) -> AnyView {
    let id = comment.id;
    let video = comment.video;
    let top = comment.parent.is_none();
    let body = RwSignal::new(comment.body.clone());
    let edited = RwSignal::new(comment.edited);
    let editing = RwSignal::new(false);
    let replying = RwSignal::new(false);
    let replies = RwSignal::new(Vec::<Comment>::new());
    let replies_next = RwSignal::new(None::<String>);
    let replies_shown = RwSignal::new(false);
    let reply_count = RwSignal::new(comment.replies);
    let channel = routes::channel(&comment.author.handle);
    let age = youcraft_i18n::when::ago(
        Language::current(),
        comment.created_at,
        RenderTime::current(),
    );
    let load_replies = move |after: Option<String>| {
        act::<comment::Replies>(OfComment { comment: id, after }, move |page| {
            if let Ok(page) = page {
                extend(replies, replies_next, page);
            }
        });
    };
    let toggle_replies = move |_| {
        let shown = !replies_shown.get_untracked();
        replies_shown.set(shown);
        if shown && replies.with_untracked(Vec::is_empty) {
            load_replies(None);
        }
    };
    let reply_parent = comment.parent.unwrap_or(id);
    let into_thread = Callback::new(move |reply: Comment| {
        replies_shown.set(true);
        replies.update(|replies| replies.push(reply));
        reply_count.update(|n| *n = n.saturating_add(1));
    });
    let reply_posted = Callback::new(move |reply: Comment| {
        replying.set(false);
        thread.unwrap_or(into_thread).run(reply);
    });
    let reply_removed = Callback::new(move |gone: CommentId| {
        replies.update(|replies| replies.retain(|c| c.id != gone));
        reply_count.update(|n| *n = n.saturating_sub(1));
    });
    let author = comment.author.clone();
    let by_creator = comment.by_creator;
    view! {
        <article class="comment" class:reply=!top>
            <a class="avatar" class:avatar-small=!top href=channel.clone() aria-hidden="true" tabindex="-1">
                <Face channel=author.clone() size=if top { Size::Regular } else { Size::Small }/>
            </a>
            <div class="comment-main">
                <p class="comment-byline">
                    <a class="comment-author" class:by-creator=by_creator href=channel>
                        <bdi>{format!("@{}", author.handle)}</bdi>
                    </a>
                    <span class="comment-age">{age}</span>
                    <Show when=move || edited.get()>
                        <span class="comment-age">{tr("(edited)")}</span>
                    </Show>
                </p>
                <Show when=move || editing.get() fallback=move || view! { <p class="comment-body" dir="auto">{move || body.get()}</p> }>
                    <EditForm id body edited editing/>
                </Show>
                <div class="comment-toolbar">
                    <LikeButton id likes=comment.likes liked=comment.liked/>
                    <button class="pill quiet small" type="button" on:click=move |_| replying.set(true)>{tr("Reply")}</button>
                    <CommentMenu comment=comment.clone() owned editing removed/>
                </div>
                <Show when=move || replying.get()>
                    <Composer video parent=Some(reply_parent) posted=reply_posted
                        cancel=Some(Callback::new(move |()| replying.set(false)))/>
                </Show>
                {top.then(|| view! {
                    <Show when=move || { reply_count.get() > 0 }>
                        <button class="replies-toggle" class:open=move || replies_shown.get() type="button"
                            aria-expanded=move || if replies_shown.get() { "true" } else { "false" } on:click=toggle_replies>
                            <Glyph icon=Icon::Expand/>
                            <span>{move || count(reply_count.get(), "{n} reply", "{n} replies")}</span>
                        </button>
                    </Show>
                    <Show when=move || replies_shown.get()>
                        <div class="replies">
                            <For each=move || replies.get() key=|reply| reply.id let:reply>
                                <CommentView comment=reply owned removed=reply_removed thread=Some(into_thread)/>
                            </For>
                            <Show when=move || replies_next.get().is_some()>
                                <button class="pill quiet small" type="button"
                                    on:click=move |_| load_replies(replies_next.get_untracked())>
                                    {tr("Show more replies")}
                                </button>
                            </Show>
                        </div>
                    </Show>
                }.into_any())}
            </div>
        </article>
    }
    .into_any()
}

/// The form that changes what a comment says.
#[component]
fn EditForm(
    id: CommentId,
    body: RwSignal<String>,
    edited: RwSignal<bool>,
    editing: RwSignal<bool>,
) -> impl IntoView {
    let draft = RwSignal::new(body.get_untracked());
    let save = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let edit = Edit {
            comment: id,
            body: draft.get_untracked(),
        };
        act::<comment::Update>(edit, move |answer| {
            if let Ok(comment) = answer {
                body.set(comment.body);
                edited.set(comment.edited);
                editing.set(false);
            }
        });
    };
    view! {
        <form class="composer-main" on:submit=save>
            <textarea class="composer-input" rows="2" aria-label=tr("Edit")
                prop:value=move || draft.get() on:input=move |event| draft.set(browser::clipped(&event, BODY_LIMIT))></textarea>
            <div class="composer-buttons">
                <button class="pill quiet" type="button" on:click=move |_| editing.set(false)>{tr("Cancel")}</button>
                <button class="pill primary" type="submit" disabled=move || draft.get().trim().is_empty()>{tr("Save")}</button>
            </div>
        </form>
    }
}

/// A comment's like button and its count.
#[component]
fn LikeButton(id: CommentId, likes: u64, liked: bool) -> impl IntoView {
    let likes = RwSignal::new(likes);
    let liked = RwSignal::new(liked);
    let toggle = move |_| {
        let (before, before_likes) = (liked.get_untracked(), likes.get_untracked());
        liked.set(!before);
        likes.set(if before {
            before_likes.saturating_sub(1)
        } else {
            before_likes.saturating_add(1)
        });
        act::<comment::Like>(
            Liking {
                comment: id,
                liked: !before,
            },
            move |answer| match answer {
                Ok(now) => {
                    liked.set(now.liked);
                    likes.set(now.likes);
                }
                Err(_) => {
                    liked.set(before);
                    likes.set(before_likes);
                }
            },
        );
    };
    view! {
        <button class="icon-button small" type="button" aria-label=tr("Like")
            aria-pressed=move || if liked.get() { "true" } else { "false" } on:click=toggle>
            <Glyph icon=Icon::Like/>
        </button>
        <span class="comment-likes">{move || (likes.get() > 0).then(|| compact(likes.get()))}</span>
    }
}

/// The ⋮ menu of a comment: what the reader may do to it.
#[component]
fn CommentMenu(
    comment: Comment,
    owned: bool,
    editing: RwSignal<bool>,
    removed: Callback<CommentId>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let (id, handle) = (comment.id, comment.author.handle.clone());
    let can_block = owned && !comment.editable;
    if !comment.editable && !comment.deletable && !can_block {
        return ().into_any();
    }
    let delete = move |_| {
        open.set(false);
        act::<comment::Delete>(Which { comment: id }, move |answer| {
            if answer.is_ok() || answer == Err(ApiError::NotFound) {
                removed.run(id);
            }
        });
    };
    let block = move |_| {
        open.set(false);
        let barring = Barring {
            handle: handle.clone(),
            blocked: true,
        };
        act::<channel::Block>(barring, move |answer| {
            if answer.is_ok() {
                removed.run(id);
            }
        });
    };
    view! {
        <div class="popup-anchor">
            <button class="icon-button small" type="button" aria-label=tr("Action menu") aria-haspopup="menu"
                on:click=move |_| open.update(|open| *open = !*open)>
                <Glyph icon=Icon::More/>
            </button>
            <Show when=move || open.get()>
                <div class="menu-backdrop" on:click=move |_| open.set(false)></div>
                <div class="popup" role="menu">
                    {comment.editable.then(|| view! {
                        <button class="menu-entry" role="menuitem" type="button"
                            on:click=move |_| { editing.set(true); open.set(false); }>
                            <Glyph icon=Icon::Edit/><span>{tr("Edit")}</span>
                        </button>
                    })}
                    {comment.deletable.then(|| view! {
                        <button class="menu-entry" role="menuitem" type="button" on:click=delete>
                            <Glyph icon=Icon::Delete/><span>{tr("Delete")}</span>
                        </button>
                    })}
                    {can_block.then(|| view! {
                        <button class="menu-entry" role="menuitem" type="button" on:click=block.clone()>
                            <Glyph icon=Icon::Block/><span>{tr("Block from your channel")}</span>
                        </button>
                    })}
                </div>
            </Show>
        </div>
    }
    .into_any()
}
