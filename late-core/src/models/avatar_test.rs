use dartboard_core::{Canvas, Pos, RgbColor};

use super::avatar::Avatar;
use super::profile::Profile;
use crate::test_utils::{create_test_user, test_db};

#[test]
fn avatar_crop_keeps_positions_colours_and_whole_wide_glyphs() {
    let mut board = Canvas::with_size(40, 40);
    let red = RgbColor::new(255, 0, 80);
    board.put_glyph_colored(Pos { x: 4, y: 3 }, '@', red);
    board.put_glyph(Pos { x: 2, y: 3 }, '界'); // crosses the left edge
    board.put_glyph(Pos { x: 6, y: 3 }, '界'); // crosses the right edge
    board.put_glyph(Pos { x: 4, y: 4 }, '界'); // wholly inside
    let avatar = Avatar::crop(&board, Pos { x: 3, y: 3 }, 4, 2).unwrap();
    let glyph = avatar.canvas().glyph_at(Pos { x: 1, y: 0 }).unwrap();
    assert_eq!((glyph.ch, glyph.fg), ('@', Some(red)));
    assert_eq!((avatar.width(), avatar.height()), (4, 2));
    assert!(avatar.canvas().glyph_at(Pos { x: 0, y: 0 }).is_none());
    assert!(avatar.canvas().glyph_at(Pos { x: 3, y: 0 }).is_none());
    assert_eq!(
        avatar.canvas().glyph_at(Pos { x: 1, y: 1 }).unwrap().width,
        2
    );
    board.put_glyph(Pos { x: 4, y: 3 }, 'X');
    assert_eq!(
        avatar.canvas().glyph_at(Pos { x: 1, y: 0 }).unwrap().ch,
        '@'
    );
}

#[test]
fn avatar_sizes_and_empty_or_out_of_bounds_frames_are_validated() {
    let mut board = Canvas::with_size(40, 40);
    board.put_glyph(Pos { x: 0, y: 0 }, '@');
    for width in 1..=32 {
        for height in 1..=32 {
            let avatar = Avatar::crop(&board, Pos { x: 0, y: 0 }, width, height).unwrap();
            assert_eq!((avatar.width(), avatar.height()), (width, height));
        }
    }
    for size in [0, 33, usize::MAX] {
        assert!(Avatar::crop(&board, Pos { x: 0, y: 0 }, size, 4).is_err());
        assert!(Avatar::crop(&board, Pos { x: 0, y: 0 }, 4, size).is_err());
    }
    assert!(Avatar::crop(&board, Pos { x: 39, y: 39 }, 4, 2).is_err());
    assert!(
        Avatar::crop(
            &board,
            Pos {
                x: usize::MAX,
                y: 0
            },
            4,
            2
        )
        .is_err()
    );
    assert!(
        Avatar::crop(
            &board,
            Pos {
                x: 0,
                y: usize::MAX
            },
            4,
            2
        )
        .is_err()
    );
    assert!(Avatar::crop(&board, Pos { x: 4, y: 4 }, 4, 2).is_err());
    board.put_glyph(Pos { x: 39, y: 39 }, '@');
    assert!(Avatar::crop(&board, Pos { x: 36, y: 38 }, 4, 2).is_ok());
    assert!(Avatar::crop(&board, Pos { x: 36, y: 39 }, 4, 2).is_err());
    assert!(Avatar::crop(&board, Pos { x: 37, y: 38 }, 4, 2).is_err());
    assert!(Avatar::crop(&board, Pos { x: 39, y: 39 }, 1, 1).is_ok());
}

#[tokio::test]
async fn legacy_square_avatars_keep_their_dimensions_and_artwork() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "legacy-avatar").await;
    let client = test.db.get().await.unwrap();
    let mut legacy = Canvas::with_size(16, 16);
    legacy.put_glyph_colored(Pos { x: 0, y: 15 }, '@', RgbColor::new(255, 0, 80));
    client
        .execute(
            "INSERT INTO user_avatars (user_id, size, canvas) VALUES ($1, 16, $2)",
            &[&user.id, &serde_json::to_value(&legacy).unwrap()],
        )
        .await
        .unwrap();
    let avatar = Avatar::load(&client, user.id).await.unwrap().unwrap();
    assert_eq!((avatar.width(), avatar.height()), (16, 16));
    assert_eq!(avatar.canvas(), &legacy);
    let profiles = Profile::list_by_user_ids(&client, &[user.id])
        .await
        .unwrap();
    assert_eq!(profiles[&user.id].avatar, Some(avatar));
}

#[tokio::test]
async fn avatar_persists_replaces_and_clears_only_for_its_owner() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "avatar-owner").await;
    let other = create_test_user(&test.db, "avatar-other").await;
    let client = test.db.get().await.unwrap();
    assert!(Avatar::load(&client, user.id).await.unwrap().is_none());
    let mut board = Canvas::with_size(32, 32);
    board.put_glyph(Pos { x: 0, y: 0 }, '@');
    let first = Avatar::crop(&board, Pos { x: 0, y: 0 }, 32, 4).unwrap();
    Avatar::save(&client, user.id, Some(&first)).await.unwrap();
    assert_eq!(
        Profile::load(&client, user.id).await.unwrap().avatar,
        Some(first)
    );
    assert!(
        Profile::load(&client, other.id)
            .await
            .unwrap()
            .avatar
            .is_none()
    );
    board.put_glyph(Pos { x: 0, y: 0 }, '#');
    let replacement = Avatar::crop(&board, Pos { x: 0, y: 0 }, 1, 32).unwrap();
    Avatar::save(&client, user.id, Some(&replacement))
        .await
        .unwrap();
    assert_eq!(
        Profile::load_with_chip_balance(&client, user.id)
            .await
            .unwrap()
            .profile
            .avatar,
        Some(replacement)
    );
    Avatar::save(&client, user.id, None).await.unwrap();
    assert!(Avatar::load(&client, user.id).await.unwrap().is_none());
}
