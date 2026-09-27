# Epilogue Playback product review

Reviewed on 2026-09-27. Epilogue lists Playback 1.10.0, released 2026-05-25,
as its latest stable desktop release. This document treats Playback as a product
reference. Cartridge Studio uses its own Rust implementation and does not bundle
Playback, its emulator cores, firmware, artwork or service integrations.
The maintained [project roadmap](roadmap.md) is the source of current priorities;
this review records the product comparison that informed the first public beta.

## Current comparison

| Area | Cartridge Studio first beta | Playback reference | Decision |
| --- | --- | --- | --- |
| ROM preservation | Two matching reads, raw passes, checksums, reports and exact offline identity across three reader families | ROM backup and integrity checking | Keep Cartridge Studio's evidence-first workflow |
| Playing a dump | External emulator playtests are development tools | Integrated libretro cores plus external-emulator workflow | Add a configurable external-emulator launcher first |
| Saves | Not implemented | Backup, restore, autosync, vault and read/write verification | Highest-priority hardware feature after ROM paths |
| GBA on GB Operator | Two-pass native ROM reads physically qualified against known GBxCart dumps | Supported | Complete for read-only standard linear ROMs |
| RTC | Not implemented | RTC data options and host-clock fallback for dead batteries | Add after save formats and transactional restore |
| Flash carts | Only explicit, electrically qualified profiles | Broad homebrew flashcart support | Expand one identified board at a time; never infer writability from a ROM |
| Game identity | Offline exact hash/size catalog with optional cached artwork | Enriched catalog, localized metadata and visual themes | Add descriptions/localization without weakening exact identity |
| Camera | Not implemented | Photo export, scaling and live emulator use | Add photo extraction after safe save reads |
| Collection | Backup library and operation history | Collection and save vault | Build a local collection view over retained reports |
| Achievements, cheats, rewind, shaders | Not implemented | Integrated emulation features | Leave to the user's emulator initially; consider libretro later |

## Product order

1. **Launch the current verified dump.** Let the user choose an emulator per
   platform, validate the executable and argument template, and launch only after
   a successful read. Keep the ROM in Cartridge Studio's library and show a clear
   error when the emulator exits early or its path changes.
2. **Safe save backup and restore.** Model save memory separately from ROMs. Read
   twice, identify the save technology and size, retain the existing cartridge
   save before restore, write in technology-specific units, read it back twice,
   and produce a recovery report. Default to manual sync.
3. **RTC support.** Preserve raw RTC data with timestamp and format metadata.
   Offer host-clock substitution only during emulation; never silently rewrite a
   cartridge clock or save.
4. **Device ownership and settings.** Give each physical serial number one
   operation owner and device-specific preferences. Automatic mode should remain
   an explicit ambiguity error when more than one usable reader is connected.
5. **Camera, collection and richer metadata.** Add read-only camera photo export,
   then collection filters, descriptions and theme colors from licensed sources.
6. **Integrated emulation extras.** Save states, rewind, shaders, achievements,
   cheats and controller mapping are valuable, but should follow reliable data
   preservation and external-emulator launching.

## UI and reliability practices to adopt now

- Filter settings and actions by the connected reader, cartridge family and
  qualified capabilities. Disabled actions must state what is missing.
- Give each reader a clear owner while an operation is active, and return it on
  completion, cancellation or window exit.
- Block Play when there is no verified ROM or configured emulator. Do not launch
  a partial read.
- Keep long operations visible with a single status, progress, cancel action and
  retained recovery location. Notifications should describe the next action.
- Keep reader, game and operation details stable while layouts change. Prevent
  truncated labels, stale dialogs and flickering artwork when cached data loads.
- Test resizing, language-length expansion, multi-reader ambiguity, disconnection,
  cancellation and settings reload in both native frontends.
- Offer native Wayland behavior and portable amd64/arm64 packages as separate,
  reproducible release targets.

## Sources

- [Playback 1.10.0 release notes](https://www.epilogue.co/changelog/playback/1.10.0)
- [Playback 1.9.0 release notes](https://www.epilogue.co/changelog/playback/1.9.0)
- [Playback changelog](https://www.epilogue.co/changelog/playback)
- [Playback settings](https://www.epilogue.co/support/customization/playback-settings)
- [Playback emulator integration](https://www.epilogue.co/support/customization/emulator-integration)
- [What is the GB Operator?](https://www.epilogue.co/support/getting-started/what-is-gb-operator)
- [GB Operator FAQ](https://www.epilogue.co/support/hardware/gb-operator-faq)
- [Community reverse-engineering notes](https://github.com/jaames/gb-operator-reverse-engineering)
