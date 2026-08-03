# macOS — status

Staging folder for macOS store assets (icons, screenshots) before they move to
`apps/desktop/macappstore/` or App Store Connect.

## Done

- [x] Dual-build: MAS (sandboxed) + direct-sale (unsandboxed), signed with correct identities
- [x] Security-scoped bookmarks wired for NAS/external libraries under the MAS sandbox
- [x] AAC codec question closed — decoder dropped, ADR 0028
- [x] GPL/MAS question mitigated — `similarity-worker` excluded from MAS build
- [x] Real notarized direct-sale DMG (Developer ID, stapled, Gatekeeper-verified)
- [x] Real signed MAS `.pkg` (3rd Party Mac Developer certs)
- [x] Self-hosted update endpoint live, verified end to end
- [x] App Store Connect record, bundle ID, provisioning profile
- [x] App Store icon (1024×1024, no alpha) + screenshots (2560×1600)
- [x] Export compliance key (`ITSAppUsesNonExemptEncryption`) baked into `Info.plist`
- [x] Build 2 uploaded, validated, submitted for Apple review
- [x] Storefront live on the website with a real screenshot, real Stripe price

## Not done yet

- [ ] Waiting on Apple's review decision
- [ ] Manual QA pass (audio playback, NAS reconnect under sandbox, keyboard nav, crash recovery, installer double-click) — never actually run
- [ ] End-to-end purchase → email → activate → trial-to-full test — never actually run
- [ ] Rotate the App Store Connect app-specific password (used directly a few times this session)
