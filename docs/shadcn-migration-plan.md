# Migration Plan: UI to shadcn

This should be a **full component-and-styling migration**, not another pass that wraps custom UI in shadcn components.

The audit found that the current app still has custom pickers, disclosures, confirmations, form layouts, status messages, and controls. Nearly **5,800 lines of legacy CSS** also override shadcn's appearance. That explains issues visible in the screenshot, including centered conversation rows and duplicate tab underlines.

I'll retain the installed **Radix-backed, neutral New York configuration** and make shadcn the default throughout the desktop app.

## Migration Scope

| Area | Planned Replacement |
|---|---|
| Sessions/Bots navigation | `Tabs` with retained panel state |
| Sidebar and conversation rows | `Sidebar` components, menu actions, badges, properly aligned multiline content |
| New session and related actions | `ButtonGroup` + `DropdownMenu` |
| Conversation toolbar | `Button`, `Toggle`, `Tooltip`, responsive action grouping |
| Search fields and composer | `InputGroup`, its input/textarea components, addons and buttons |
| Messages and attachments | `Message`, `Bubble`, `Attachment`, `Marker` |
| Transcript scrolling | `MessageScroller`, connected to existing history restoration, search and jump behavior |
| Context sections and tool runs | Complete `Accordion`/`Collapsible` compositions, including their triggers |
| Errors, warnings and notices | `Alert`, `AlertTitle`, `AlertDescription`; appropriate status announcements |
| Approval and question surfaces | `Card`, `Alert`, `FieldSet`, selection controls and explicit `Button` actions |
| Model picker | `Popover` + `Command`, preserving current search and ranking |
| Theme picker | Controlled `Command` selection with reversible preview and explicit commit |
| Settings, bots and memory forms | `FieldGroup`, `Field`, `Select`, validation components, proper action sections |
| Memory views and revision history | `Tabs` + `Accordion`/`Collapsible` |
| Delete/reset/discard confirmations | `AlertDialog`, replacing custom confirmation blocks and `window.confirm()` |
| All modal interiors | Proper `DialogHeader`, title, description, content and footer composition |
| Empty/loading states | `Empty`, `Skeleton`, `Spinner` |
| Counts, state labels and hints | `Badge`, `Tooltip`, `Kbd`, `Separator` |
| Resizable columns and narrow layouts | `Resizable` panels and responsive `Sheet` |
| File lists, audit, permissions, watches | `Item`/`Card` compositions with standard actions and feedback |
| Markdown tables and code/diff controls | `Table`, standard action controls and disclosures |

The current shadcn documentation includes the chat primitives above, so the conversation UI is part of the migration too.

## Implementation Order

1. **Fix the foundation.** Consolidate theme tokens and radius settings, connect dark mode to the effective palette, and establish a deliberate CSS layer/reset policy. Fix portal stacking and dialog sizing conflicts.

2. **Replace shared compositions.** Complete dialogs, collapsibles, menus, tooltips, fields, button groups and pickers. Remove wrappers that merely preserve the old markup.

3. **Migrate every application surface.** Work through the shell, sidebar, transcript, approvals, context, bot management, settings and secondary dialogs. Remove each surface's legacy visual overrides in the same change.

4. **Retire the old component styling.** Remove the HeroUI-inspired override block and other obsolete control styles. Keep only necessary application layout, document typography, avatar rendering and security-specific markings. Separate PDF typography before removing shared legacy rules.

5. **Repair and expand verification.** Update tests to assert behavior and accessibility rather than exact old class strings or `<strong>`/`<summary>` tags. Repair outdated fixtures, including the model driver's folder-picker stub, without weakening filesystem validation.

## What Stays Custom

- Electron's native application menu and file dialogs.
- Agent/session state, IPC, persistence and approval decisions.
- Safe Markdown rendering, numbered diff content and provenance markings.
- Seeded bot avatars and file-tree hierarchy/loading logic.
- Print-specific PDF layout.

These exceptions concern **application behavior and specialized content**, not a reason to keep custom buttons, cards or alerts.

## Completion Criteria

- Every applicable UI pattern uses a real shadcn composition, not just an outer wrapper.
- Legacy CSS no longer overrides ordinary component variants.
- Keyboard navigation, nested overlays, long content, narrow layouts and all themes work.
- Drafts, selections, folds, scroll position and existing preferences survive the migration.
- Security tests, deterministic Electron workflows, typecheck, build and PDF checks pass.