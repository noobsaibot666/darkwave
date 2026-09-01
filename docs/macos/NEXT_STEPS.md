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

## Apple accounts (see repo `CLAUDE.md` § "Apple release accounts & IDs" for the full list)

- Developer Apple ID for App Store Connect / notarization / Transporter: **`alan.creative@icloud.com`** (not the machine's `alanxalves@me.com` login — that mismatch caused repeated notarization/upload 401s).
- Team `RD7UU4Z3D2` · App Store Connect app ID `6797313803`.
- Notary keychain profile `darkwave-notary`; app-specific passwords expire → regenerate under `alan.creative@icloud.com` and re-run `xcrun notarytool store-credentials darkwave-notary --apple-id alan.creative@icloud.com --team-id RD7UU4Z3D2`.
- Bump `CFBundleVersion` in `apps/desktop/src-tauri/Info.plist` for every App Store upload.
