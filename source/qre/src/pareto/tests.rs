// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::pareto::{ParetoFrontier, ParetoFrontier3D, ParetoItem2D, ParetoItem3D};

struct Point2D {
    x: f64,
    y: f64,
}

impl ParetoItem2D for Point2D {
    type Objective1 = f64;
    type Objective2 = f64;

    fn objective1(&self) -> Self::Objective1 {
        self.x
    }

    fn objective2(&self) -> Self::Objective2 {
        self.y
    }
}

#[test]
fn test_update_frontier() {
    let mut frontier: ParetoFrontier<Point2D> = ParetoFrontier::new();
    let p1 = Point2D { x: 1.0, y: 5.0 };
    frontier.insert(p1);
    assert_eq!(frontier.0.len(), 1);
    let p2 = Point2D { x: 2.0, y: 4.0 };
    frontier.insert(p2);
    assert_eq!(frontier.0.len(), 2);
    let p3 = Point2D { x: 1.5, y: 6.0 };
    frontier.insert(p3);
    assert_eq!(frontier.0.len(), 2);
    let p4 = Point2D { x: 3.0, y: 3.0 };
    frontier.insert(p4);
    assert_eq!(frontier.0.len(), 3);
    let p5 = Point2D { x: 2.5, y: 2.0 };
    frontier.insert(p5);
    assert_eq!(frontier.0.len(), 3);
}

#[test]
fn test_iter_frontier() {
    let mut frontier: ParetoFrontier<Point2D> = ParetoFrontier::new();
    frontier.insert(Point2D { x: 1.0, y: 5.0 });
    frontier.insert(Point2D { x: 2.0, y: 4.0 });

    let mut iter = frontier.iter();
    let p = iter.next().expect("Has element");
    assert!((p.x - 1.0).abs() <= f64::EPSILON);
    assert!((p.y - 5.0).abs() <= f64::EPSILON);

    let p = iter.next().expect("Has element");
    assert!((p.x - 2.0).abs() <= f64::EPSILON);
    assert!((p.y - 4.0).abs() <= f64::EPSILON);

    assert!(iter.next().is_none());

    // Test IntoIterator for &ParetoFrontier
    for p in &frontier {
        assert!(p.x > 0.0);
    }
}

struct Point3D {
    x: f64,
    y: f64,
    z: f64,
}

impl ParetoItem3D for Point3D {
    type Objective1 = f64;
    type Objective2 = f64;
    type Objective3 = f64;

    fn objective1(&self) -> Self::Objective1 {
        self.x
    }

    fn objective2(&self) -> Self::Objective2 {
        self.y
    }

    fn objective3(&self) -> Self::Objective3 {
        self.z
    }
}

#[test]
fn test_update_frontier_3d() {
    let mut frontier: ParetoFrontier3D<Point3D> = ParetoFrontier3D::new();

    // p1: 1, 5, 5
    let p1 = Point3D {
        x: 1.0,
        y: 5.0,
        z: 5.0,
    };
    frontier.insert(p1);
    assert_eq!(frontier.0.len(), 1);

    // p2: 2, 6, 6 (dominated by p1)
    let p2 = Point3D {
        x: 2.0,
        y: 6.0,
        z: 6.0,
    };
    frontier.insert(p2);
    assert_eq!(frontier.0.len(), 1);

    // p3: 0.5, 6, 6 (not dominated, x makes it unique)
    let p3 = Point3D {
        x: 0.5,
        y: 6.0,
        z: 6.0,
    };
    frontier.insert(p3);
    assert_eq!(frontier.0.len(), 2);

    // p4: 1, 4, 4 (dominates p1, should remove p1 and add p4)
    // p1 (1,5,5). p4 (1,4,4). p4 <= p1? 1<=1, 4<=5, 4<=5. Yes.
    // p3 (0.5,6,6). p4 (1,4,4). p4 <= p3? 1<=0.5 False.
    // Result: p1 removed, p4 added. p3 remains.
    let p4 = Point3D {
        x: 1.0,
        y: 4.0,
        z: 4.0,
    };
    frontier.insert(p4);
    assert_eq!(frontier.0.len(), 2);

    // Verify content (generic check, not order specific)
    let points: Vec<(f64, f64, f64)> = frontier.iter().map(|p| (p.x, p.y, p.z)).collect();

    // Should contain p3 and p4
    assert!(
        points
            .iter()
            .any(|p| (p.0 - 0.5).abs() < 1e-9 && (p.1 - 6.0).abs() < 1e-9)
    );
    assert!(
        points
            .iter()
            .any(|p| (p.0 - 1.0).abs() < 1e-9 && (p.1 - 4.0).abs() < 1e-9)
    );
}

// #[test]
// fn perf_test_create_frontier() {
//     let points = (0..1000000)
//         .map(|i| Point2D {
//             key: i,
//             x: rand::random::<f64>() * 10000.0,
//             y: rand::random::<f64>() * 10000.0,
//         })
//         .collect::<Vec<_>>();
//     let timer = std::time::Instant::now();
//     let frontier: ParetoFrontier<Point2D> = points.into_iter().collect();
//     let duration = timer.elapsed();
//     println!("Time taken to create frontier: {:?}", duration);
//     println!("Final frontier size: {}", frontier.0.len());
// }
