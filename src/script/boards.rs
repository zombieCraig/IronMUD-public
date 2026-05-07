// src/script/boards.rs
//
// Bulletin board (gen_board parity) Rhai surface. Mirrors src/script/mail.rs:
// posts surface as rhai maps so scripts don't need a registered BoardPost
// type. Per-board access gating lives on the board item's ItemData fields
// — these helpers operate on raw post data; the calling script is
// responsible for admin checks.

use crate::db::Db;
use crate::BoardPost;
use rhai::Engine;
use std::sync::Arc;

pub fn register(engine: &mut Engine, db: Arc<Db>) {
    // ========== Query ==========

    // count_board_posts_for(board_vnum) -> i64
    let cloned_db = db.clone();
    engine.register_fn("count_board_posts_for", move |board_vnum: String| -> i64 {
        cloned_db.count_board_posts(&board_vnum).unwrap_or(0) as i64
    });

    // list_board_posts(board_vnum) -> Array of post summary maps
    // Each map: { id, author, subject, posted_at }
    let cloned_db = db.clone();
    engine.register_fn("list_board_posts", move |board_vnum: String| -> rhai::Array {
        match cloned_db.get_board_posts(&board_vnum) {
            Ok(posts) => posts
                .into_iter()
                .map(|p| {
                    let mut map = rhai::Map::new();
                    map.insert("id".into(), rhai::Dynamic::from(p.id.to_string()));
                    map.insert("author".into(), rhai::Dynamic::from(p.author));
                    map.insert("subject".into(), rhai::Dynamic::from(p.subject));
                    map.insert("posted_at".into(), rhai::Dynamic::from(p.posted_at));
                    rhai::Dynamic::from(map)
                })
                .collect(),
            Err(_) => rhai::Array::new(),
        }
    });

    // get_board_post_by_index(board_vnum, index_1based) -> full post map or ()
    // Map: { id, author, subject, body, posted_at }
    let cloned_db = db.clone();
    engine.register_fn(
        "get_board_post_by_index",
        move |board_vnum: String, index: i64| -> rhai::Dynamic {
            if index < 1 {
                return rhai::Dynamic::UNIT;
            }
            match cloned_db.get_board_posts(&board_vnum) {
                Ok(posts) => {
                    let idx = (index - 1) as usize;
                    if idx >= posts.len() {
                        return rhai::Dynamic::UNIT;
                    }
                    let p = &posts[idx];
                    let mut map = rhai::Map::new();
                    map.insert("id".into(), rhai::Dynamic::from(p.id.to_string()));
                    map.insert("author".into(), rhai::Dynamic::from(p.author.clone()));
                    map.insert("subject".into(), rhai::Dynamic::from(p.subject.clone()));
                    map.insert("body".into(), rhai::Dynamic::from(p.body.clone()));
                    map.insert("posted_at".into(), rhai::Dynamic::from(p.posted_at));
                    rhai::Dynamic::from(map)
                }
                Err(_) => rhai::Dynamic::UNIT,
            }
        },
    );

    // ========== Mutate ==========

    // add_board_post(board_vnum, author, subject, body) -> bool
    // Stores a post with eviction-on-overflow according to
    // ItemData.board_max_messages on the board prototype (None defaults
    // to Db::DEFAULT_BOARD_MAX_MESSAGES = 60).
    let cloned_db = db.clone();
    engine.register_fn(
        "add_board_post",
        move |board_vnum: String, author: String, subject: String, body: String| -> bool {
            let max = cloned_db
                .get_item_by_vnum(&board_vnum)
                .ok()
                .flatten()
                .and_then(|item| item.board_max_messages);
            let post = BoardPost::new(board_vnum, author, subject, body);
            cloned_db.store_board_post(post, max).is_ok()
        },
    );

    // delete_board_post_by_index(board_vnum, index_1based) -> bool
    let cloned_db = db.clone();
    engine.register_fn(
        "delete_board_post_by_index",
        move |board_vnum: String, index: i64| -> bool {
            if index < 1 {
                return false;
            }
            match cloned_db.get_board_posts(&board_vnum) {
                Ok(posts) => {
                    let idx = (index - 1) as usize;
                    if idx >= posts.len() {
                        return false;
                    }
                    cloned_db.delete_board_post(&posts[idx].id).unwrap_or(false)
                }
                Err(_) => false,
            }
        },
    );

    // get_board_post_author_at(board_vnum, index_1based) -> String
    // Convenience for "is the requester the author?" gating in the
    // remove command — avoids parsing a map back out in Rhai.
    let cloned_db = db.clone();
    engine.register_fn(
        "get_board_post_author_at",
        move |board_vnum: String, index: i64| -> String {
            if index < 1 {
                return String::new();
            }
            match cloned_db.get_board_posts(&board_vnum) {
                Ok(posts) => {
                    let idx = (index - 1) as usize;
                    posts.get(idx).map(|p| p.author.clone()).unwrap_or_default()
                }
                Err(_) => String::new(),
            }
        },
    );
}
