//! Exercise the reset image's explicit partition and current-schema erase path.
use super::*;
use crate::test_support::{InMemoryFlash, test_block_on};

type Part = InMemoryFlash<1_048_576, 4096, 4>;
type Store = Storage<BlockingAsync<Part>, 8, 7, 7, 2>;
const START: usize = 0xB0000;
const END: usize = 0xD0000;

async fn boot(part: Part, reset: bool) -> Store {
    Box::pin(Store::new(
        async_flash_wrapper(part),
        &StorageConfig {
            start_addr: START,
            num_sectors: 32,
            clear_storage: reset,
            clear_layout: false,
        },
    ))
    .await
}

#[test]
fn explicit_reset_clears_current_settings_and_bonds_within_right_partition_only() {
    // InMemoryFlash constructs a full-size array before moving it to Rc. Give
    // the fixture its own stack so plain `cargo test` is also supported.
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            test_block_on(async {
                let mut part = Part::new();
                // Fill every byte outside the settings partition, including both the
                // installed application/SoftDevice and the bootloader/settings pages.
                embedded_storage::nor_flash::NorFlash::write(&mut part, 0, &vec![0xA5; START])
                    .unwrap();
                embedded_storage::nor_flash::NorFlash::write(
                    &mut part,
                    END as u32,
                    &vec![0x5A; 1_048_576 - END],
                )
                .unwrap();
                let mut storage = boot(part.clone(), false).await;
                let items = [
                    StorageItem::DefaultLayer(3),
                    StorageItem::LayoutOption(42),
                    StorageItem::BehaviorConfig((&config::BehaviorConfig::default()).into()),
                    StorageItem::ConnectionType(ConnectionType::Ble),
                    StorageItem::Keymap {
                        layer: 0,
                        row: 7,
                        col: 3,
                        action: crate::k!(A),
                    },
                    StorageItem::Encoder {
                        layer: 0,
                        idx: 0,
                        action: EncoderAction::new(crate::k!(B), crate::k!(C)),
                    },
                    StorageItem::MacroChunk {
                        idx: 0,
                        bytes: [0x55; MACRO_CHUNK_SIZE],
                    },
                    StorageItem::UserData {
                        slot: 0,
                        data: heapless::Vec::from_slice(&[1, 2, 3, 4]).unwrap(),
                    },
                    StorageItem::PeerAddress(PeerAddress::new(0, true, [1; 6])),
                    StorageItem::BondInfo(ProfileInfo::default()),
                    StorageItem::ActiveBleProfile(1),
                ];
                let mut keys = Vec::new();
                for item in items {
                    keys.push(item.clone().split().0);
                    storage.put(item).await.unwrap();
                }
                drop(storage);
                let mut storage = boot(part.clone(), false).await;
                for key in &keys {
                    assert!(
                        storage.fetch(*key).await.unwrap().is_some(),
                        "normal boot must preserve {key:?}"
                    );
                }
                drop(storage);

                let mut storage = boot(part.clone(), true).await;
                for key in &keys {
                    assert!(
                        storage.fetch(*key).await.unwrap().is_none(),
                        "reset must erase {key:?}"
                    );
                }
                assert!(
                    matches!(storage.fetch(StorageKey::StorageConfig).await.unwrap(),
            Some(StorageValue::StorageConfig(schema)) if schema == SCHEMA_HASH)
                );
                // Hand back to the normal firmware: changes made after the reset must
                // survive a subsequent boot instead of causing another implicit wipe.
                storage.put(StorageItem::DefaultLayer(2)).await.unwrap();
                drop(storage);
                let mut storage = boot(part.clone(), false).await;
                assert!(matches!(
                    storage.fetch(StorageKey::DefaultLayer).await.unwrap(),
                    Some(StorageValue::DefaultLayer(2))
                ));

                let mut lower = vec![0; START];
                let mut upper = vec![0; 1_048_576 - END];
                embedded_storage::nor_flash::ReadNorFlash::read(&mut part, 0, &mut lower).unwrap();
                embedded_storage::nor_flash::ReadNorFlash::read(&mut part, END as u32, &mut upper)
                    .unwrap();
                assert!(lower.iter().all(|b| *b == 0xA5));
                assert!(upper.iter().all(|b| *b == 0x5A));
            })
        })
        .unwrap()
        .join()
        .unwrap();
}
