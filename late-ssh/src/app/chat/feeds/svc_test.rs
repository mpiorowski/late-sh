use tokio::sync::watch;
use uuid::Uuid;

use super::{FeedSnapshot, parse_feed, publish_list};

#[test]
fn an_older_list_finishing_late_does_not_replace_a_newer_one() {
    let (tx, rx) = watch::channel(FeedSnapshot::default());
    let user_id = Some(Uuid::now_v7());
    let list = |seq| FeedSnapshot {
        user_id,
        seq,
        ..FeedSnapshot::default()
    };
    publish_list(&tx, list(2));
    publish_list(&tx, list(1));
    assert_eq!(rx.borrow().seq, 2);
    publish_list(&tx, list(3));
    assert_eq!(rx.borrow().seq, 3);
}

#[test]
fn parse_feed_reads_rss_items() {
    let xml = r#"
        <rss><channel><title>Blog</title>
        <item><title>Hello</title><link>/hello</link><guid>1</guid><description><![CDATA[<p>Hi</p>]]></description></item>
        </channel></rss>
    "#;
    let feed = parse_feed("https://example.com/feed.xml", xml).expect("feed");
    assert_eq!(feed.title, "Blog");
    assert_eq!(feed.entries[0].url, "https://example.com/hello");
    assert_eq!(feed.entries[0].summary, "Hi");
}

#[test]
fn parse_feed_strips_entity_encoded_html() {
    let xml = r#"
        <rss><channel><title>Blog</title>
        <item><title>T</title><link>/x</link><guid>1</guid><description>&lt;table border=0&gt;&lt;tr&gt;&lt;td&gt;Hello world&lt;/td&gt;&lt;/tr&gt;&lt;/table&gt;</description></item>
        </channel></rss>
    "#;
    let feed = parse_feed("https://example.com/feed.xml", xml).expect("feed");
    assert_eq!(feed.entries[0].summary, "Hello world");
}

#[test]
fn parse_feed_reads_atom_entries() {
    let xml = r#"
        <feed><title>Atom</title>
        <entry><title>Post</title><id>tag:post</id><link href="https://example.com/post" /></entry>
        </feed>
    "#;
    let feed = parse_feed("https://example.com/feed", xml).expect("feed");
    assert_eq!(feed.entries[0].url, "https://example.com/post");
}
