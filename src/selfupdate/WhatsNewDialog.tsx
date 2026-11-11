// What's new after a self-update (#189): the launcher's own release notes, opened once on the first
// start of the new version, the way a product's Patch Notes open once after its update.
import { NotesSections, Text } from "../product/PatchNotes";
import { parseNotes } from "../product/notesMarkdown";
import { Dialog } from "../ui/Dialog";
import { whatsNewTitle } from "./copy";

/** The version whose notes were last shown. */
export const WHATS_NEW_SEEN_KEY = "launcher.whatsNewSeen";

/** True once per version: reading it marks `version` seen. Storage failures show nothing. */
export function takeWhatsNew(version: string): boolean {
  try {
    if (localStorage.getItem(WHATS_NEW_SEEN_KEY) === version) return false;
    localStorage.setItem(WHATS_NEW_SEEN_KEY, version);
    return true;
  } catch {
    return false;
  }
}

export function WhatsNewDialog({
  version,
  notes,
  onReleasePage,
  onClose,
}: {
  version: string;
  notes: string;
  onReleasePage(): void;
  onClose(): void;
}) {
  const parts = parseNotes(notes);
  const key = `launcher-${version}`;
  return (
    <Dialog
      title={whatsNewTitle(version)}
      onClose={onClose}
      actions={
        <>
          <button type="button" className="btn primary" onClick={onClose}>
            Close
          </button>
          <button type="button" className="btn ghost" onClick={onReleasePage}>
            Release page
          </button>
        </>
      }
    >
      <div className="su-notes">
        {parts.summary && (
          <p className="notes-summary">
            <Text inlines={parts.summary} />
          </p>
        )}
        <NotesSections releaseKey={key} sections={parts.sections} />
        {parts.truncated && <p className="notes-truncated">These notes are too long to show in full.</p>}
      </div>
    </Dialog>
  );
}
