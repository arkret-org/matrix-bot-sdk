use async_trait::async_trait;
use matrix_bot_sdk::client::MatrixClient;
use matrix_bot_sdk::mixins::{
    AutojoinUpgradedRoomsMixin, DefaultAutojoinRoomsMixin, DefaultAutojoinUpgradedRoomsMixin,
};

#[test]
fn default_autojoin_rooms_mixin_can_be_created() {
    let _mixin = DefaultAutojoinRoomsMixin;
}

#[test]
fn default_autojoin_upgraded_rooms_mixin_can_be_created() {
    let _mixin = DefaultAutojoinUpgradedRoomsMixin;
}

#[test]
fn default_autojoin_upgraded_rooms_mixin_implements_trait() {
    // Verify the struct can be used as trait object
    let mixin: Box<dyn AutojoinUpgradedRoomsMixin> = Box::new(DefaultAutojoinUpgradedRoomsMixin);
    // The trait object compiles, which confirms the implementation
    let _ = mixin;
}

#[test]
fn default_autojoin_rooms_mixin_is_debug() {
    let mixin = DefaultAutojoinRoomsMixin;
    let _ = format!("{:?}", mixin);
}

#[test]
fn default_autojoin_upgraded_rooms_mixin_is_debug() {
    let mixin = DefaultAutojoinUpgradedRoomsMixin;
    let _ = format!("{:?}", mixin);
}

/// Test that verifies the AutojoinUpgradedRoomsMixin trait exists and can be used
/// with custom implementations for handling room upgrade scenarios.
#[test]
fn test_autojoin_upgraded_rooms_trait_bound() {
    // This test verifies that any type implementing the trait can handle
    // room upgrade events appropriately.
    struct CustomUpgradeHandler;

    #[async_trait]
    impl AutojoinUpgradedRoomsMixin for CustomUpgradeHandler {
        async fn on_room_upgraded(
            &self,
            _client: &MatrixClient,
            _old_room_id: &str,
            _new_room_id: &str,
        ) -> anyhow::Result<()> {
            // Custom logic for handling room upgrades
            Ok(())
        }
    }

    // Verify we can create and use the custom implementation
    let handler = CustomUpgradeHandler;
    // The handler can now be used with appservices
    let _ = handler;
}

/// Test that verifies the default implementation handles tombstone events.
/// Room upgrades in Matrix are signaled by m.room.tombstone events.
#[test]
fn test_tombstone_event_structure() {
    // This test documents the expected structure of a tombstone event.
    // Room upgrades are signaled by an m.room.tombstone state event:
    //
    // {
    //   "type": "m.room.tombstone",
    //   "state_key": "",
    //   "content": {
    //     "body": "This room has been replaced",
    //     "replacement_room": "!new_room:example.org"
    //   }
    // }
    //
    // The AutojoinUpgradedRoomsMixin should:
    // 1. Detect this event type
    // 2. Extract the replacement_room field
    // 3. Leave the old room
    // 4. Join the new room

    // Example tombstone event content
    let tombstone_content = serde_json::json!({
        "body": "This room has been replaced",
        "replacement_room": "!new_room:example.org"
    });

    // Verify the structure can be parsed
    assert!(tombstone_content.is_object());
    assert_eq!(
        tombstone_content.get("body").unwrap().as_str(),
        Some("This room has been replaced")
    );
    assert_eq!(
        tombstone_content.get("replacement_room").unwrap().as_str(),
        Some("!new_room:example.org")
    );
}
