import type { DroppedFile } from '../../shared/drops'
import { FileGlyph } from './FileGlyph'

/**
 * A dropped file shown where it was sent, as the composer shows it while staged: the thumbnail of
 * a picture or the glyph of its type, its name, and the marker that stands for it in the line. It
 * has no remove button, since what was sent is no longer the person's to take off.
 */
export function DroppedChip({ marker, file }: { marker?: string; file: DroppedFile }): React.JSX.Element {
  return (
    <span className="attachment-chip dropped shown" data-test="sent-dropped-chip">
      {file.thumbnail
        ? <img className="attachment-thumb" src={file.thumbnail} alt="" />
        : <FileGlyph name={file.name} />}
      <span className="attachment-label" data-tooltip={file.name}>{file.name}</span>
      {marker && <span className="attachment-marker">{marker}</span>}
    </span>
  )
}
