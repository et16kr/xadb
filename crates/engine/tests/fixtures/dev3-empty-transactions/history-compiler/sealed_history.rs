#![allow(dead_code)]
use super::resident::History;
fn probe() -> History {
    History { digests: [[0; 32]; 4], count: 1 }
}
