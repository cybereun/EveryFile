import type { FolderRecord } from "../../lib/types";

interface Props {
  folders: FolderRecord[];
  onAdd: () => void;
}

export function FolderSidebar({ folders, onAdd }: Props) {
  return (
    <aside aria-label={"\uB4F1\uB85D \uD3F4\uB354"}>
      <div className="sidebar-heading">
        <h2>{"\uB4F1\uB85D \uD3F4\uB354"}</h2>
        {folders.length > 0 && (
          <button
            type="button"
            onClick={onAdd}
            aria-label={"\uD3F4\uB354 \uCD94\uAC00"}
          >
            +
          </button>
        )}
      </div>
      {folders.length === 0 ? (
        <div className="folder-empty-state">
          <p>
            {
              "\uC120\uD0DD\uD55C \uD3F4\uB354\uB9CC \uC0C9\uC778\uB429\uB2C8\uB2E4."
            }
          </p>
          <button type="button" onClick={onAdd}>
            {"\uD3F4\uB354 \uC120\uD0DD"}
          </button>
        </div>
      ) : (
        folders.map((folder) => (
          <button className="folder-row" key={folder.id} type="button">
            <span>{folder.displayName}</span>
            <span>{folder.documentCount.toLocaleString()}</span>
          </button>
        ))
      )}
    </aside>
  );
}
