# Orion UI Design Specification

This document provides a complete pixel-level specification for the Orion mail application UI. A programmer should be able to recreate the entire interface from this document alone.

---

## Table of Contents

1. [Window Configuration](#window-configuration)
2. [Theme System](#theme-system)
3. [Typography](#typography)
4. [Spacing System](#spacing-system)
5. [Layout Structure](#layout-structure)
6. [Components](#components)
7. [Views](#views)
8. [Icons](#icons)
9. [States & Interactions](#states--interactions)
10. [Keyboard Shortcuts](#keyboard-shortcuts)

---

## Window Configuration

| Property | Value |
|----------|-------|
| Width | 1200px |
| Height | 800px |
| Title | "Orion" |
| Titlebar | Native macOS titlebar with traffic lights |

---

## Theme System

Orion uses `gpui-component`'s theme system in **Dark Mode**. All colors reference theme variables.

### Color Tokens

| Token | Usage |
|-------|-------|
| `theme.background` | Main content background |
| `theme.foreground` | Primary text color |
| `theme.muted_foreground` | Secondary/subdued text |
| `theme.border` | Borders and dividers |
| `theme.secondary` | Sidebar background, keyboard hints |
| `theme.secondary_foreground` | Text on secondary backgrounds |
| `theme.list` | List item background (default) |
| `theme.list_active` | Selected list item background |
| `theme.list_hover` | Hovered list item background |
| `theme.list_active_border` | Left accent border on selected items |
| `theme.primary` | Primary accent (unread dots, avatars) |
| `theme.primary_foreground` | Text on primary backgrounds |
| `theme.danger` | Error state background |
| `theme.danger_foreground` | Error state text |
| `theme.transparent` | Transparent background |

### Custom Colors

| Color | Value | Usage |
|-------|-------|-------|
| Search highlight | `hsla(50°, 90%, 50%, 0.4)` | Yellow highlight for search matches |
| Modal backdrop | `hsla(0°, 0%, 0%, 0.5)` | Semi-transparent overlay |

---

## Typography

### Font Sizes

| Size Token | Description |
|------------|-------------|
| `text_xs` | Extra small (labels, counts, badges) |
| `text_sm` | Small (list items, body text) |
| `text_lg` | Large (headings, titles) |

### Font Weights

| Weight | Value | Usage |
|--------|-------|-------|
| `NORMAL` | 400 | Default body text |
| `MEDIUM` | 500 | Selected items, dates |
| `SEMIBOLD` | 600 | Unread items, section headers |
| `BOLD` | 700 | Page titles, app branding |

---

## Spacing System

All spacing uses a 4px base unit.

### Padding Values

| Token | Value |
|-------|-------|
| `px` | 1px |
| `p_1` / `px_1` / `py_1` | 4px |
| `p_2` / `px_2` / `py_2` | 8px |
| `p_3` / `px_3` / `py_3` | 12px |
| `p_4` / `px_4` / `py_4` | 16px |
| `py_1p5` | 6px |
| `pt_8` | 32px |
| `pb_4` | 16px |

### Gap Values

| Token | Value |
|-------|-------|
| `gap_1` | 4px |
| `gap_2` | 8px |
| `gap_3` | 12px |
| `gap_4` | 16px |
| `gap_8` | 32px |

### Border Radius

| Token | Value |
|-------|-------|
| `rounded(px(4))` | 4px |
| `rounded_md` | Medium (default for items) |
| `rounded_lg` | Large (modals, cards) |
| `rounded_full` | 50% (circles, avatars) |

---

## Layout Structure

### Root Layout

```
┌─────────────────────────────────────────────────────────────┐
│ Window: 1200px × 800px                                      │
├────────────────┬────────────────────────────────────────────┤
│                │  Header (search box, right-aligned)        │
│    Sidebar     │  h: auto, px: 16px, py: 8px, border-b      │
│    w: 240px    ├────────────────────────────────────────────┤
│    bg: secondary│                                           │
│    border-r: 1px│  Content Area (flex: 1)                   │
│                │  - ThreadListView OR                       │
│                │  - ThreadView OR                           │
│                │  - SearchResultsView                       │
│                │                                            │
└────────────────┴────────────────────────────────────────────┘
```

### Root Container

| Property | Value |
|----------|-------|
| Display | Flex |
| Direction | Row |
| Size | 100% width, 100% height |
| Background | `theme.background` |
| Text Color | `theme.foreground` |
| Position | Relative (for overlays) |

---

## Components

### 1. Sidebar

**Container:**
| Property | Value |
|----------|-------|
| Width | 240px (fixed) |
| Height | 100% |
| Background | `theme.secondary` |
| Border Right | 1px solid `theme.border` |
| Display | Flex column |

**Structure:**
```
┌──────────────────────────┐
│ Header (Branding)        │  pt: 32px, pb: 16px, px: 12px
├──────────────────────────┤
│ Accounts Section         │  px: 8px, pb: 8px, border-b
│ ┌──────────────────────┐ │
│ │ "ACCOUNTS" label     │ │  py: 4px, px: 4px
│ ├──────────────────────┤ │
│ │ All Accounts item    │ │
│ │ Account 1 item       │ │
│ │ Account 2 item       │ │
│ │ + Add Account        │ │
│ └──────────────────────┘ │
├──────────────────────────┤
│ Labels Section           │  px: 8px, py: 4px, flex: 1
│ ┌──────────────────────┐ │
│ │ Inbox                │ │
│ │ Sent                 │ │
│ │ Drafts               │ │
│ │ Starred              │ │
│ │ ...                  │ │
│ └──────────────────────┘ │
├──────────────────────────┤
│ Footer (Sync)            │  border-t, px: 12px, py: 8px
└──────────────────────────┘
```

**Branding Header:**
| Element | Style |
|---------|-------|
| Container | pt: 32px, pb: 16px, px: 12px, flex row, gap: 8px |
| "Orion" text | `text_lg`, `BOLD`, `theme.foreground` |
| "Mail" label | `text_xs`, `theme.muted_foreground` |

**Accounts Section Header:**
| Element | Style |
|---------|-------|
| Container | py: 4px, px: 4px |
| "ACCOUNTS" text | `text_xs`, `SEMIBOLD`, `theme.muted_foreground` |

---

### 2. SidebarItem (Label Item)

Used for mailbox labels (Inbox, Sent, Drafts, etc.)

**Layout:**
| Property | Value |
|----------|-------|
| Width | 100% |
| Padding | 12px horizontal, 6px vertical |
| Margin | 1px vertical |
| Border Radius | Medium |
| Border Left | 2px |
| Display | Flex row, justify-between, items-center |
| Cursor | Pointer |

**Left Content (flex row, gap: 8px):**
- Icon: Size Small
- Label: `text_sm`

**Right Content:**
- Unread count (if > 0): `text_xs`, `theme.muted_foreground`

**States:**

| State | Background | Border Color | Text Color | Font Weight |
|-------|------------|--------------|------------|-------------|
| Default | transparent | transparent | `muted_foreground` | NORMAL |
| Hover | `list_hover` | transparent | `muted_foreground` | NORMAL |
| Selected | `list_active` | `list_active_border` | `foreground` | MEDIUM |

**Label Icons:**

| Label | Icon |
|-------|------|
| Inbox | `IconName::Inbox` |
| Sent | `IconName::ArrowRight` |
| Drafts | `IconName::File` |
| Trash | `IconName::Delete` |
| Spam | `IconName::TriangleAlert` |
| Starred | `IconName::Star` |
| Important | `IconName::Bell` |
| All Mail | `IconName::Folder` |
| Other | `IconName::Folder` |

---

### 3. AccountItem

Used for individual account entries in the sidebar.

**Layout:**
| Property | Value |
|----------|-------|
| Width | 100% |
| Padding | 12px horizontal, 6px vertical |
| Margin | 1px vertical |
| Border Radius | Medium |
| Border Left | 2px |
| Display | Flex row, justify-between, items-center |
| Cursor | Pointer |

**Left Content (flex row, gap: 8px):**

**Avatar Circle:**
| Property | Value |
|----------|-------|
| Size | 20px × 20px |
| Border Radius | Full (circle) |
| Background | `theme.primary` |
| Text | First letter of email (uppercase) |
| Text Style | `text_xs`, `SEMIBOLD`, `theme.primary_foreground` |

**Email Text:**
| Property | Value |
|----------|-------|
| Font Size | `text_sm` |
| Max Width | 140px |
| Overflow | Text ellipsis |
| Font Weight | MEDIUM (selected), NORMAL (default) |
| Color | `foreground` (selected), `muted_foreground` (default) |

**Right Content:**
- Spinner (XSmall) when syncing
- Unread count (`text_xs`, `muted_foreground`) when not syncing and count > 0

**States:** Same as SidebarItem

---

### 4. AllAccountsItem

Special item for "All Accounts" unified view.

**Layout:** Identical to SidebarItem

**Content:**
- Icon: `IconName::Inbox`, Size Small
- Label: "All Accounts", `text_sm`
- Right: Total unread count (if > 0)

**States:** Same as SidebarItem

---

### 5. Add Account Button

**Layout:**
| Property | Value |
|----------|-------|
| Padding | 8px horizontal, 4px vertical |
| Margin | 4px horizontal, 4px top |
| Border Radius | Medium |
| Display | Flex row, gap: 8px |
| Cursor | Pointer |

**Content:**
- Icon: `IconName::Plus`, XSmall, `muted_foreground`
- Text: "Add Account", `text_sm`, `muted_foreground`

**States:**
| State | Background |
|-------|------------|
| Default | transparent |
| Hover | `list_hover` |

---

### 6. Sidebar Footer (Sync)

**Layout:**
| Property | Value |
|----------|-------|
| Border Top | 1px solid `theme.border` |
| Padding | 12px horizontal, 8px vertical |
| Display | Flex row, justify-between, items-center |

**Left Content:**
- Last sync time: `text_xs`, `muted_foreground`

**Right Content:**
- Sync button: Ghost variant, Small size
  - Icon: Custom `RefreshCw` icon
  - Label: "Sync" or "Syncing..." (when active)
  - Loading state shows animated spinner

---

### 7. SearchBox

**Layout:**
| Property | Value |
|----------|-------|
| Width | 280px |
| Display | Flex row, items-center |
| Gap | 4px |

**Components:**

1. **Search Icon:** `IconName::Search`, Small, `muted_foreground`

2. **Input Field:** gpui-component Input
   - Appearance: false (no border)
   - Cleanable: true (X button to clear)
   - Width: 100%
   - Placeholder: "Search mail..."

3. **Keyboard Hint** (only when input is empty):
   | Property | Value |
   |----------|-------|
   | Content | "/" |
   | Padding | 4px horizontal, 1px vertical |
   | Background | `theme.border` |
   | Border Radius | 4px |
   | Font | `text_xs`, `muted_foreground` |

**Behavior:**
- 150ms debounce on input
- Enter: Submit search
- Escape: Cancel and clear

---

### 8. ThreadListItem

Individual email thread row in the list.

**Layout:**
| Property | Value |
|----------|-------|
| Height | 40px |
| Width | 100% |
| Padding | 12px horizontal |
| Gap | 8px |
| Border Bottom | 1px solid `theme.border` |
| Display | Flex row, items-center |
| Cursor | Pointer |

**Column Structure:**
```
┌─────┬────────────────┬─────────────────────────────┬──────────┐
│ Dot │ Sender (180px) │ Subject + Snippet (flex: 1) │ Date     │
│ 6px │                │                             │          │
└─────┴────────────────┴─────────────────────────────┴──────────┘
```

**For Unified View (All Accounts), add Account column:**
```
┌─────┬────────────────┬──────────────┬─────────────────────┬──────────┐
│ Dot │ Sender (180px) │ Account      │ Subject + Snippet   │ Date     │
│ 6px │                │ (140px)      │ (flex: 1)           │          │
└─────┴────────────────┴──────────────┴─────────────────────┴──────────┘
```

**Unread Indicator Dot:**
| Property | Value |
|----------|-------|
| Size | 6px × 6px |
| Border Radius | Full (circle) |
| Background | `theme.primary` (unread), transparent (read) |
| Flex Shrink | 0 |

**Sender Column:**
| Property | Value |
|----------|-------|
| Width | 180px |
| Min Width | 0 (allows shrink) |
| Display | Flex row, items-center, gap: 4px |
| Overflow | Text ellipsis |

- Sender name: `text_sm`, `SEMIBOLD` (unread) / `NORMAL` (read)
- Message count badge (if > 1): `text_xs`, `muted_foreground`, in parentheses

**Account Column (Unified View only):**
| Property | Value |
|----------|-------|
| Width | 140px |
| Font | `text_xs`, `muted_foreground` |
| Overflow | Text ellipsis |

**Subject + Snippet Column:**
| Property | Value |
|----------|-------|
| Flex | 1 |
| Min Width | 0 |
| Overflow | Text ellipsis |
| Display | Inline |

- Subject: `text_sm`, `SEMIBOLD` (unread) / `NORMAL` (read), `foreground`
- Separator: " - "
- Snippet: `text_sm`, `muted_foreground`

**Date Column:**
| Property | Value |
|----------|-------|
| Flex Shrink | 0 |
| Font | `text_xs` |
| Weight | `MEDIUM` (unread), `NORMAL` (read) |
| Color | `foreground` (unread), `muted_foreground` (read) |

**Date Formatting:**
| Condition | Format | Example |
|-----------|--------|---------|
| Today | HH:MM | "14:30" |
| This week | Day name | "Mon" |
| Older | Month Day | "Dec 15" |

**States:**
| State | Background |
|-------|------------|
| Default | `theme.list` |
| Hover | `theme.list_hover` |
| Selected | `theme.list_active` |

---

### 9. SearchResultItem

Identical to ThreadListItem with added **search highlighting**.

**Highlighting:**
- Query matches in subject and snippet are highlighted
- Highlight style: Yellow background at 40% opacity
- Color: `hsla(50°, 90%, 50%, 0.4)`
- Case-insensitive matching
- Overlapping highlights are merged
- Uses GPUI `StyledText` with `HighlightStyle`

---

### 10. ShortcutsHelp Modal

Full-screen overlay showing keyboard shortcuts.

**Backdrop:**
| Property | Value |
|----------|-------|
| Position | Absolute, full screen |
| Background | `hsla(0°, 0%, 0%, 0.5)` |
| Display | Flex, center |

**Modal Container:**
| Property | Value |
|----------|-------|
| Background | `theme.background` |
| Border | 1px solid `theme.border` |
| Border Radius | Large |
| Shadow | Large |
| Padding | 16px |

**Header:**
| Property | Value |
|----------|-------|
| Padding Bottom | 12px |
| Margin Bottom | 12px |
| Border Bottom | 1px solid `theme.border` |
| Display | Flex, justify-between |

- Title: "Keyboard Shortcuts", `text_lg`, `BOLD`, `foreground`
- Subtitle: "Press Escape or ? to close", `text_sm`, `muted_foreground`

**Content Grid:**
| Property | Value |
|----------|-------|
| Display | Flex row |
| Gap | 32px |

**Category Section:**
| Property | Value |
|----------|-------|
| Min Width | 200px |
| Display | Flex column |
| Gap | 4px |

- Category name: `text_sm`, `SEMIBOLD`, `foreground`
- Header: pb: 4px, mb: 4px, border-bottom: 1px

**Shortcut Item:**
| Property | Value |
|----------|-------|
| Display | Flex row |
| Gap | 12px |

- Key box:
  - Min Width: 70px
  - Padding: 8px horizontal, 1px vertical
  - Background: `theme.secondary`
  - Border Radius: 4px
  - Font: `text_xs`, `MEDIUM`, `secondary_foreground`
- Description: `text_sm`, `muted_foreground`

---

## Views

### 1. ThreadListView

Main inbox/label view showing list of threads.

**Layout:**
| Property | Value |
|----------|-------|
| Size | 100% width, 100% height |
| Display | Flex column |

**Header:**
| Property | Value |
|----------|-------|
| Width | 100% |
| Padding | 16px horizontal, 12px vertical |
| Background | `theme.background` |
| Border Bottom | 1px solid `theme.border` |
| Display | Flex row, justify-between, items-center |

- Label name: `text_lg`, `BOLD`, `foreground`
- Stats: `text_sm`, `muted_foreground`
  - Format: "X messages" or "X messages, Y unread"

**List Container:**
| Property | Value |
|----------|-------|
| Flex | 1 |
| Background | `theme.list` |
| Overflow | Hidden (uses virtual scrolling) |

- Uses `v_virtual_list` from gpui-component
- Item height: 40px
- Vertical scrollbar

**Loading State (Skeleton):**
- 8 skeleton rows
- Each row: 40px height, px: 16px, py: 8px
- Border bottom: 1px
- Skeleton elements:
  - Line 1: 120px wide, 16px tall (sender placeholder)
  - Line 1: flex-1, 16px tall (subject placeholder)
  - Line 2: 280px wide, 14px tall (snippet placeholder)

**Empty State:**
- Centered flex column
- Primary: "No emails yet", `text_sm`, `muted_foreground`
- Secondary: "Sync your inbox to get started", `text_xs`, `muted_foreground`

**Error State:**
- Centered, padding: 16px
- Error box: padding: 16px, `theme.danger` background, rounded-lg
- Error text: `text_sm`, `theme.danger_foreground`

---

### 2. ThreadView

Single thread/conversation view.

**Header:**
| Property | Value |
|----------|-------|
| Width | 100% |
| Padding | 16px horizontal, 12px vertical |
| Background | `theme.background` |
| Border Bottom | 1px solid `theme.border` |
| Display | Flex row, items-center |
| Gap | 12px |

**Back Button:**
- Icon: `IconName::ArrowLeft`, Small, `foreground`
- Variant: Ghost
- Cursor: Pointer

**Title Section (flex column, flex: 1):**
- Subject: `text_lg`, `BOLD`, `foreground`, text-ellipsis
- Message count: `text_xs`, `muted_foreground`
  - "1 message" (singular) or "X messages" (plural)

**Action Buttons (flex row, gap: 4px):**
All buttons: Ghost variant, cursor pointer

| Action | Icon | Size | Color |
|--------|------|------|-------|
| Archive | Custom `Archive` | Small | `muted_foreground` |
| Star | `IconName::Star` | Small | `muted_foreground` |
| Read/Unread | Custom `MailOpen` | Small | `muted_foreground` |
| Delete | `IconName::Delete` | Small | `muted_foreground` |

**Content Area:**
- Contains rendered email messages
- Scrollable

---

### 3. SearchResultsView

Search results display.

**Layout:**
| Property | Value |
|----------|-------|
| Size | 100% |
| Display | Flex column |
| Background | `theme.background` |

**Header:**
| Property | Value |
|----------|-------|
| Width | 100% |
| Padding | 16px horizontal, 12px vertical |
| Background | `theme.background` |
| Border Bottom | 1px solid `theme.border` |
| Display | Flex row, justify-between |

- Title: `text_lg`, `BOLD`, `foreground`
- Format: "X results for \"query\""

**Results Container:**
| Property | Value |
|----------|-------|
| Flex | 1 |
| Background | `theme.list` |

- Uses virtual scrolling
- Item height: 40px
- Vertical scrollbar

**Loading State:**
- Centered flex column, gap: 8px
- Spinner: Medium size
- Text: "Searching...", `text_sm`, `muted_foreground`

**Empty State:**
- Centered flex column, gap: 8px
- Primary: "No results found", `text_sm`, `muted_foreground`
- Secondary: "Try different search terms", `text_xs`, `muted_foreground`

---

## Icons

### Built-in Icons (gpui-component)

| Icon Name | Usage |
|-----------|-------|
| `IconName::Inbox` | Inbox label, All Accounts |
| `IconName::ArrowRight` | Sent label |
| `IconName::ArrowLeft` | Back button |
| `IconName::File` | Drafts label |
| `IconName::Delete` | Trash label, Delete action |
| `IconName::TriangleAlert` | Spam label |
| `IconName::Star` | Starred label, Star action |
| `IconName::Bell` | Important label |
| `IconName::Folder` | Generic folder/label |
| `IconName::Search` | Search box |
| `IconName::Plus` | Add account |

### Custom Icons (assets/icons/)

| Icon | File | Usage |
|------|------|-------|
| `Archive` | `icons/archive.svg` | Archive action button |
| `MailOpen` | `icons/mail-open.svg` | Read/Unread toggle |
| `RefreshCw` | `icons/refresh-cw.svg` | Sync button |

### Icon Sizes

| Size | Pixels | Usage |
|------|--------|-------|
| XSmall | ~12px | Add account button icon |
| Small | ~16px | All other icons |
| Medium | ~20px | Loading spinners |

---

## States & Interactions

### List Item States

| State | Trigger | Visual Change |
|-------|---------|---------------|
| Default | None | `list` background |
| Hover | Mouse over | `list_hover` background |
| Selected | Click/keyboard | `list_active` background, left border accent |
| Unread | Data state | Bold text, colored dot |

### Button States

| State | Visual Change |
|-------|---------------|
| Default | Ghost appearance |
| Hover | Subtle background change |
| Active | Pressed appearance |
| Loading | Animated spinner |

### Focus States

- Search input shows focus ring when active
- List items receive focus via keyboard navigation

---

## Keyboard Shortcuts

### Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move to next thread |
| `k` / `↑` | Move to previous thread |
| `Enter` | Open selected thread |
| `Escape` | Go back / close |
| `g i` | Go to Inbox |
| `g s` | Go to Sent |
| `g d` | Go to Drafts |
| `g a` | Go to All Mail |

### Actions

| Key | Action |
|-----|--------|
| `e` | Archive thread |
| `s` | Toggle star |
| `#` | Delete thread |
| `Shift+I` | Mark as read |
| `Shift+U` | Mark as unread |
| `r` | Refresh/sync |

### Search

| Key | Action |
|-----|--------|
| `/` | Focus search box |
| `Escape` | Clear search, unfocus |

### Help

| Key | Action |
|-----|--------|
| `?` | Toggle shortcuts help |

---

## Component Sizes Reference

| Component | Dimensions |
|-----------|------------|
| Window | 1200px × 800px |
| Sidebar | 240px wide |
| Thread item height | 40px |
| Search result height | 40px |
| Search box width | 280px |
| Avatar | 20px × 20px |
| Unread dot | 6px × 6px |
| Sender column | 180px |
| Account column | 140px |

---

## Implementation Notes

### Required Dependencies

- `gpui` (0.2.2) - Core UI framework
- `gpui-component` (0.5.0) - Theme system, components, icons
- `rust-embed` - Asset embedding for custom icons

### Theme Initialization

```rust
gpui_component::init(cx);
Theme::change(ThemeMode::Dark, None, cx);
```

### Root Wrapper Requirement

The app root must be wrapped in gpui-component's `Root` element for Input components to work:

```rust
cx.new(|cx| Root::new(app_entity, window, cx))
```

### Virtual Scrolling

Thread lists use gpui-component's `v_virtual_list` for performance with large lists. Item height must be constant (40px).

### Text Highlighting

For search result highlighting, use GPUI's `StyledText` API:

```rust
let highlight = HighlightStyle {
    background_color: Some(hsla(50./360., 0.9, 0.5, 0.4)),
    ..Default::default()
};
StyledText::new(text).with_highlights(vec![(range, highlight)])
```
