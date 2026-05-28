use crate::traits::*;
use glam::I16Vec2;

fn check_physical_collision<E1, E2>(one: &E1, other: &E2) -> Option<(ColliderType, ColliderType)>
where
    E1: Entity,
    E2: Entity,
{
    if one.previous_position() == one.position() {
        return None;
    }

    let path = one.previous_position().as_i16vec2() - one.position().as_i16vec2();
    let other_position = other.position().as_i16vec2();
    let other_hit_box = other.hit_box();

    let offsets: Box<dyn Iterator<Item = I16Vec2>> = if path.x != 0 {
        let slope = path.y as f32 / path.x as f32;
        let xs: Box<dyn Iterator<Item = i16>> = if path.x > 0 {
            Box::new(0..=path.x)
        } else {
            Box::new(path.x..=0)
        };
        Box::new(xs.map(move |x| I16Vec2::new(x, (slope * x as f32).round() as i16)))
    } else {
        let ys: Box<dyn Iterator<Item = i16>> = if path.y > 0 {
            Box::new(0..=path.y)
        } else {
            Box::new(path.y..=0)
        };
        Box::new(ys.map(|y| I16Vec2::new(0, y)))
    };

    for offset in offsets {
        for (&point, &one_collider_type) in one.hit_box().iter() {
            let hit_box_position = (one.position() + point).as_i16vec2() + offset;
            if hit_box_position.x < other_position.x || hit_box_position.y < other_position.y {
                continue;
            }
            let g_point = (hit_box_position - other_position).as_u16vec2();
            if let Some(&other_collider_type) = other_hit_box.get(&g_point) {
                return Some((one_collider_type, other_collider_type));
            }
        }
    }

    None
}

fn check_granular_phase_collision<E1, E2>(
    one: &E1,
    other: &E2,
) -> Option<(ColliderType, ColliderType)>
where
    E1: Entity,
    E2: Entity,
{
    for (&point, &one_collider_type) in one.hit_box().iter() {
        let hit_box_position = one.position() + point;
        let other_position = other.position();
        if hit_box_position.x < other_position.x || hit_box_position.y < other_position.y {
            continue;
        }
        let g_point = hit_box_position - other_position;
        if let Some(&other_collider_type) = other.hit_box().get(&g_point) {
            return Some((one_collider_type, other_collider_type));
        }
    }

    None
}

pub fn check_broad_phase_collision<E1, E2>(one: &E1, other: &E2) -> bool
where
    E1: Entity,
    E2: Entity,
{
    let (s1_min, s1_max) = (
        one.previous_position(),
        one.previous_position() + one.size(),
    );
    let (o1_min, o1_max) = (
        other.previous_position(),
        other.previous_position() + other.size(),
    );

    let (s2_min, s2_max) = (one.position(), one.position() + one.size());
    let (o2_min, o2_max) = (other.position(), other.position() + other.size());

    if (s1_min.x > o1_max.x && s2_min.x > o2_max.x)
        || (o1_min.x > s1_max.x && o2_min.x > s2_max.x)
        || (s1_min.y > o1_max.y && s2_min.y > o2_max.y)
        || (o1_min.y > s1_max.y && o2_min.y > s2_max.y)
    {
        return false;
    }

    true
}

pub fn are_colliding<E1, E2>(one: &E1, other: &E2) -> Option<(ColliderType, ColliderType)>
where
    E1: Entity,
    E2: Entity,
{
    // Broad phase detection, shortcut if rects cannot intersect
    if !check_broad_phase_collision(one, other) {
        return None;
    }

    // Granular phase detection
    if let Some(colliders) = check_granular_phase_collision(one, other) {
        return Some(colliders);
    }

    // Physical path phase detection
    // This is not perfect, since we don't check if the entities crossed paths while moving,
    // but only one against the other final position. We also don't check if the entity didn't move
    // but rotated somehow. Good enough for us.
    if let Some(colliders) = check_physical_collision(one, other) {
        log::debug!(
            "Found physical collision! {}->{} hit {:#?}",
            one.previous_position(),
            one.position(),
            other.rect()
        );
        return Some(colliders);
    }

    // Do the same swapping entities.
    if let Some(colliders) = check_physical_collision(other, one) {
        log::debug!(
            "Found physical collision! {}->{} hit {:#?}",
            other.previous_position(),
            other.position(),
            one.rect()
        );
        return Some((colliders.1, colliders.0));
    }

    None
}

pub fn inelastic_collision<E1, E2>(one: &mut E1, other: &mut E2, coefficient_of_restituion: f32)
where
    E1: Entity,
    E2: Entity,
{
    one.set_position(one.previous_position());
    other.set_position(other.previous_position());

    let v2_one = if one.mass() == f32::INFINITY {
        one.velocity()
    } else if other.mass() == f32::INFINITY {
        coefficient_of_restituion * (other.velocity() - one.velocity()) + other.velocity()
    } else {
        (coefficient_of_restituion * other.mass() * (other.velocity() - one.velocity())
            + one.mass() * one.velocity()
            + other.mass() * other.velocity())
            / (one.mass() + other.mass())
    };
    let v2_other = if one.mass() == f32::INFINITY {
        coefficient_of_restituion * (one.velocity() - other.velocity()) + one.velocity()
    } else if other.mass() == f32::INFINITY {
        other.velocity()
    } else {
        // Symmetric to v2_one with the (v-v) sign flipped; without the flip,
        // equal masses converge on a single post-collision velocity.
        (coefficient_of_restituion * one.mass() * (one.velocity() - other.velocity())
            + one.mass() * one.velocity()
            + other.mass() * other.velocity())
            / (one.mass() + other.mass())
    };

    one.set_velocity(v2_one);
    other.set_velocity(v2_other);
}
