# TODO

- [x] Incremental sync should sync on all actions (read, delete, archive, etc.)
- [x] Mark as read/unread
- [x] Archive messages
- [x] Delete messages
- [x] Apply/remove labels
- [x] Star/unstar messages
- [ ] Add toast notifications for actions
- [x] Add keyboard shortcuts help overlay
- [ ] Add command palette (`Cmd+K`)
- [x] Multi-account support
- [ ] Attachments + blob storage
- [ ] Compose email
- [ ] Reply to email
- [x] Scroll on keyboard up/down nav
- [ ] Go to star and toggle start keyboard shortcuts conflict (as with any other shared non-modifier keys)
- [x] Sync improvements
  - [x] Try and minimize the rate limiting on initial sync
  - [x] Sync should be resilient to sleep/restart/etc
- [x] Sandbox thread html css
- [x] Keep mailbox up to date
- [ ] Integration with OS notifications
- [ ] Onboarding
  - [ ] Login experience
  - [ ] Sync explanation
- [ ] OS default keyboard shortcuts (quit, minimize, maximize, etc.)
- [x] Unread count badge on nav
- [x] Use total/unread counts on list headers (instead of just the in-memory counts)
- [ ] Improve plain text email rendering (horizontal padding)
- [ ] List view paging
- [x] Only half my main mailbox was synced
- [x] Sync is causing too many renders. Debounce.

## SwiftUI / iOS

- [ ] Choose minimum deployment targets (currently targeting 26.0 for macOS and iOS)
- [x] Wire up MailService FFI calls in MailBridge
- [x] Implement OAuth flow in AuthService
- [x] Wire up Add Account and Sync buttons in SidebarView
- [x] Wire up thread actions (archive, star, read/unread)
- [x] Cross-platform OAuth credentials (xcconfig for SwiftUI, symlink for GPUI)
- [ ] Test on iOS device/simulator