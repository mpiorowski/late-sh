use rand::SeedableRng;
use rand::rngs::StdRng;

use super::space::{Asteroid, Flight, Space, WIN_ALTITUDE};

#[test]
fn an_asteroid_in_the_way_costs_hull() {
    let mut flight = Space::new(2, 1);
    let mut rng = StdRng::seed_from_u64(1);
    flight.asteroids.push(Asteroid {
        x: flight.ship_x.round(),
        y: flight.ship_y.round(),
        speed: 0.0,
        glyph: '#',
    });

    flight.tick(0.1, &mut rng);
    assert_eq!(flight.hull, 1, "a rock in the same cell takes a point");
    assert!(flight.outcome.is_none());

    flight.asteroids.push(Asteroid {
        x: flight.ship_x.round(),
        y: flight.ship_y.round(),
        speed: 0.0,
        glyph: '#',
    });
    flight.tick(0.1, &mut rng);
    assert_eq!(flight.hull, 0);
    assert_eq!(
        flight.outcome,
        Some(Flight::Crashed),
        "an empty hull is the end of the flight"
    );
}

#[test]
fn sixty_seconds_of_climbing_wins() {
    let mut flight = Space::new(50, 1);
    let mut rng = StdRng::seed_from_u64(2);
    // Fly wide of everything: the point here is the clock, not the dodging.
    // Generous on iterations, because a tenth of a second is not exact in
    // binary and the altitude clock only counts whole ones.
    for _ in 0..(WIN_ALTITUDE + 5) * 15 {
        flight.asteroids.clear();
        flight.tick(0.1, &mut rng);
        if flight.outcome.is_some() {
            break;
        }
    }
    assert_eq!(flight.outcome, Some(Flight::Won));
    assert!(flight.altitude > WIN_ALTITUDE);
}

#[test]
fn the_layers_are_named_by_altitude() {
    let mut flight = Space::new(1, 1);
    let expect = [
        (0, "Troposphere"),
        (10, "Stratosphere"),
        (20, "Mesosphere"),
        (30, "Thermosphere"),
        (45, "Exosphere"),
        (60, "Space"),
    ];
    for (altitude, name) in expect {
        flight.altitude = altitude;
        assert_eq!(flight.layer(), name, "at {altitude}km");
    }
}

#[test]
fn better_thrusters_move_the_ship_further() {
    let mut slow = Space::new(1, 1);
    let mut fast = Space::new(1, 5);
    let start = slow.ship_x;
    slow.nudge(1.0, 0.0);
    fast.nudge(1.0, 0.0);
    assert!(
        fast.ship_x - start > slow.ship_x - start,
        "upgrading the engine has to be worth the alloy"
    );
}
