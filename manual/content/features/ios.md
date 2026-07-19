+++
title = "iOS features"
description = "The fully native app: widgets, notifications, Spotlight, share-sheet capture, Siri & Shortcuts, and the Action button."
weight = 14
+++

The iOS app is fully native SwiftUI with the entire Rust core running **in-process** — no server, no round trips, and everything works offline (the network is only for provider metadata, cover downloads, and whatever syncing you already use). Vaults live on-device under On My iPhone or open from Files/iCloud Drive; provider credentials live in the Keychain.

Beyond feature parity with web and desktop, iOS adds the platform integrations below.

## Coming Up widget

A Home Screen widget for [what's coming up](@/features/calendar.md#what-s-coming-up): upcoming releases and next episodes, straight from your calendar data.

{{ screenshot(platforms="ios", caption="The Coming Up widget on the Home Screen.") }}

## Notifications & reminders

Opt-in reminders for the dates you're waiting on — releases and airing episodes from your `planning` dates. Reminder preferences are device-local (they're a *you-on-this-device* choice, not vault data).

{{ screenshot(platforms="ios", caption="A release reminder.") }}

## Spotlight search

Your entities are indexed into system Spotlight: search from the Home Screen and jump straight to a detail page, without opening the app first.

{{ screenshot(platforms="ios", caption="An entity in Spotlight results.") }}

## Share-sheet capture

Found something in Safari? Share a provider page to KizunaShelf and it lands in [Quick Capture](@/features/adding.md) pre-filled — from browsing to on-the-shelf in two taps.

{{ screenshot(platforms="ios", caption="Capturing from Safari via the share sheet.") }}

## Siri, Shortcuts & the Action button

App Intents expose [Log activity](@/features/log.md) and capture to Siri and the Shortcuts app — so "log an episode" can be a sentence, a Shortcut automation, or the **Action button** on recent iPhones. Check off an episode from the couch; it's still just a line of Markdown.

{{ screenshot(platforms="ios", caption="A Log activity shortcut on the Action button.") }}

## Background downloads

Cover and asset downloads run through background `URLSession` transfers, so a batch keeps going when you leave the app.

---

The iOS app is on [TestFlight](https://testflight.apple.com/join/hE7k3sWd). One writing note honored throughout: iOS error messages speak plain English — with an in-process core there is no "server" or "request" to blame.
