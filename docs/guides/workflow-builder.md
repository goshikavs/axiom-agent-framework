# Workflow builder

Studio → Workflows (`http://127.0.0.1:3000/workflows`).

- Drag nodes from the palette onto the React Flow canvas
- Connect handles; condition edges should be labeled `true` / `false`
- Configure the selected node in the right panel
- **Validate** posts `/api/v1/workflows/validate`
- **Save draft** stores an incomplete graph
- **Publish** snapshots a version
- **Execute** starts a pinned run and links to the execution explorer
- Ctrl+S saves; Delete removes the selected node
- Import/export is JSON (untrusted on import)

Node execution status for a run is shown on the execution detail page (`data-testid=workflow-nodes`), not as a static diagram.
