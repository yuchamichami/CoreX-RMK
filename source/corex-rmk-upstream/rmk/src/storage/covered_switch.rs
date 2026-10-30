//! One-time correction of the CoreX rev1.0 right covered switch only.
//! Uses an existing user-data slot; no storage schema or pairing changes.
use crate::keymap::KeymapData;
use crate::storage::{Storage, StorageItem, StorageKey, StorageValue};
use embedded_storage_async::nor_flash::NorFlash;
use rmk_types::action::KeyAction;

const SLOT: u8 = 250;
const MARKER: &[u8] = b"covered-space-v1";

pub(super) fn eligible(board: &str, rows: usize, cols: usize, layers: usize) -> bool {
    board == "raytac-r-a14" && (rows, cols, layers) == (8, 7, 7)
}

pub(super) async fn migrate<
    F: NorFlash,
    const R: usize,
    const C: usize,
    const L: usize,
    const E: usize,
>(
    storage: &mut Storage<F, R, C, L, E>,
    data: &mut KeymapData<R, C, L, E>,
) -> Result<(), ()> {
    if (R, C, L) != (8, 7, 7) {
        return Ok(());
    }
    match storage.fetch(StorageKey::UserData(SLOT)).await? {
        Some(StorageValue::UserData(_)) => return Ok(()), // Never replace an occupied slot.
        None => {}
        _ => return Err(()),
    }
    let base = data.keymap[0][3][3];
    if base == KeyAction::No || base == crate::k!(F14) || base == crate::k!(Space) {
        // Write changed cells before the marker. A power loss can safely retry.
        for (layer, action) in [
            (0, crate::k!(Space)),
            (2, KeyAction::Transparent),
            (3, KeyAction::Transparent),
        ] {
            let old = data.keymap[layer][3][3];
            if old != action && (layer == 0 || old == KeyAction::No) {
                storage
                    .put(StorageItem::Keymap {
                        layer: layer as u8,
                        row: 3,
                        col: 3,
                        action,
                    })
                    .await
                    .map_err(|_| ())?;
                data.keymap[layer][3][3] = action;
            }
        }
    }
    // A custom assignment is preserved, and a later explicit No stays No.
    storage
        .put(StorageItem::UserData {
            slot: SLOT,
            data: heapless::Vec::from_slice(MARKER).map_err(|_| ())?,
        })
        .await
        .map_err(|_| ())
}

#[cfg(all(test, feature = "_ble", feature = "split"))]
mod tests {
    use super::*;
    use crate::config::StorageConfig;
    use crate::storage::async_flash_wrapper;
    use crate::test_support::{InMemoryFlash, test_block_on};
    use embassy_embedded_hal::adapter::BlockingAsync;
    type Part = InMemoryFlash<32768, 4096, 4>;
    type Store = Storage<BlockingAsync<Part>, 8, 7, 7, 0>;

    async fn boot(part: Part) -> Store {
        Store::new(
            async_flash_wrapper(part),
            &StorageConfig {
                start_addr: 0,
                num_sectors: 8,
                clear_storage: false,
                clear_layout: false,
            },
        )
        .await
    }

    #[test]
    fn only_rev1_right_can_run_the_migration() {
        assert!(eligible("raytac-r-a14", 8, 7, 7));
        for board in ["", "prototype", "raytac-l-rev1", "minew"] {
            assert!(!eligible(board, 8, 7, 7));
        }
        assert!(!eligible("raytac-r-a14", 4, 7, 7));
    }

    #[test]
    fn legacy_cells_change_once_and_other_saved_items_survive() {
        test_block_on(async {
            for old in [KeyAction::No, crate::k!(F14)] {
                let part = Part::new();
                let mut store = boot(part.clone()).await;
                store.put(StorageItem::ActiveBleProfile(2)).await.unwrap();
                store
                    .put(StorageItem::PeerAddress(
                        crate::split::ble::PeerAddress::new(0, true, [7; 6]),
                    ))
                    .await
                    .unwrap();
                store
                    .put(StorageItem::BondInfo(
                        crate::ble::profile::ProfileInfo::default(),
                    ))
                    .await
                    .unwrap();
                store
                    .put(StorageItem::MacroChunk {
                        idx: 0,
                        bytes: [0x55; rmk_types::constants::MACRO_CHUNK_SIZE],
                    })
                    .await
                    .unwrap();
                store
                    .put(StorageItem::Keymap {
                        layer: 0,
                        row: 7,
                        col: 3,
                        action: crate::k!(A),
                    })
                    .await
                    .unwrap();
                let mut data = KeymapData::new([[[crate::k!(B); 7]; 8]; 7]);
                data.keymap[0][3][3] = old;
                data.keymap[2][3][3] = KeyAction::No;
                data.keymap[3][3][3] = KeyAction::No;
                let before = data.keymap;
                migrate(&mut store, &mut data).await.unwrap();
                for l in 0..7 {
                    for r in 0..8 {
                        for c in 0..7 {
                            let expected = match (l, r, c) {
                                (0, 3, 3) => crate::k!(Space),
                                (2 | 3, 3, 3) => KeyAction::Transparent,
                                _ => before[l][r][c],
                            };
                            assert_eq!(data.keymap[l][r][c], expected);
                        }
                    }
                }
                drop(store);
                let mut store = boot(part).await;
                assert!(
                    matches!(store.fetch(StorageKey::Keymap {layer:0,row:3,col:3}).await.unwrap(), Some(StorageValue::KeyAction(a)) if a == crate::k!(Space))
                );
                for key in [
                    StorageKey::ActiveBleProfile,
                    StorageKey::PeerAddress(0),
                    StorageKey::BondInfo(0),
                    StorageKey::MacroChunk(0),
                    StorageKey::Keymap {
                        layer: 0,
                        row: 7,
                        col: 3,
                    },
                ] {
                    assert!(
                        store.fetch(key).await.unwrap().is_some(),
                        "preserve {key:?}"
                    );
                }
                // Later host edits must not be overridden on another boot.
                data.keymap[0][3][3] = KeyAction::No;
                migrate(&mut store, &mut data).await.unwrap();
                assert_eq!(data.keymap[0][3][3], KeyAction::No);
            }
        });
    }

    #[test]
    fn custom_assignments_and_failed_writes_are_not_marked_over() {
        test_block_on(async {
            let mut part = Part::new();
            let mut store = boot(part.clone()).await;
            let mut data = KeymapData::new([[[crate::k!(A); 7]; 8]; 7]);
            migrate(&mut store, &mut data).await.unwrap();
            assert!(
                data.keymap
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|a| *a == crate::k!(A))
            );
            data.keymap[0][3][3] = crate::k!(F14);
            // Force an actual flash write failure, then retry the same store.
            part = Part::new();
            let mut failing = boot(part.clone()).await;
            part.fail_writes(true);
            assert!(migrate(&mut failing, &mut data).await.is_err());
            assert_eq!(data.keymap[0][3][3], crate::k!(F14));
            part.fail_writes(false);
            assert!(
                failing
                    .fetch(StorageKey::UserData(SLOT))
                    .await
                    .unwrap()
                    .is_none()
            );
            migrate(&mut failing, &mut data).await.unwrap();
            assert_eq!(data.keymap[0][3][3], crate::k!(Space));
        });
    }
}
