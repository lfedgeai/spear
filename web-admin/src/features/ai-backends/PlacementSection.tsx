import type { Dispatch, SetStateAction } from 'react'

import type { AiBackendDesiredState } from '@/api/ai-backends'
import type { NodeSummary } from '@/api/types'
import { Input } from '@/components/ui/input'

import type { AiBackendEditorFormState, PlacementScope } from './AiBackendEditorForm'
import { formatNodeLabel, toggleNodeValue } from './AiBackendEditorForm'

export default function PlacementSection(props: {
  form: AiBackendEditorFormState
  setForm: Dispatch<SetStateAction<AiBackendEditorFormState>>
  nodes: NodeSummary[]
  selectedPlacementNodes: string[]
  isLoadingNodes: boolean
}) {
  const { form, setForm, nodes, selectedPlacementNodes, isLoadingNodes } = props
  return (
    <div className="space-y-3 rounded-[var(--radius)] border border-[hsl(var(--border))] p-3">
      <div>
        <div className="text-sm font-medium">Placement</div>
        <div className="text-xs text-[hsl(var(--muted-foreground))]">
          Remote backends default to all nodes. Local backends default to one node.
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1">
          <div className="text-sm font-medium">Deployment scope</div>
          <select
            className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
            value={form.placement_scope}
            onChange={(event) => {
              const scope = event.target.value as PlacementScope
              setForm((current) => ({
                ...current,
                placement_scope: scope,
                placement_node_uuid: scope === 'single_node' && nodes.length === 1 ? nodes[0].uuid : '',
                placement_node_uuids: scope === 'selected_nodes' ? current.placement_node_uuids : '',
              }))
            }}
          >
            <option value="all_nodes">All Nodes</option>
            <option value="single_node">Single Node</option>
            <option value="selected_nodes">Selected Nodes</option>
          </select>
        </div>
        <div className="space-y-1">
          <div className="text-sm font-medium">Placement state</div>
          <select
            className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
            value={form.placement_state}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                placement_state: event.target.value as AiBackendDesiredState,
              }))
            }
          >
            <option value="enabled">enabled</option>
            <option value="disabled">disabled</option>
          </select>
        </div>
      </div>

      {form.placement_scope === 'single_node' ? (
        <div className="space-y-1">
          <div className="text-sm font-medium">Node</div>
          <select
            className="h-9 w-full rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] bg-[hsl(var(--background))] px-3 text-sm"
            value={form.placement_node_uuid}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                placement_node_uuid: event.target.value,
              }))
            }
          >
            <option value="">Select a node…</option>
            {nodes.map((node) => (
              <option key={node.uuid} value={node.uuid}>
                {formatNodeLabel(node)}
              </option>
            ))}
          </select>
        </div>
      ) : null}

      {form.placement_scope === 'selected_nodes' ? (
        <div className="space-y-1">
          <div className="text-sm font-medium">Nodes</div>
          <div className="grid grid-cols-1 gap-2 rounded-[calc(var(--radius)-4px)] border border-[hsl(var(--input))] px-3 py-2">
            {nodes.map((node) => (
              <label key={node.uuid} className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={selectedPlacementNodes.includes(node.uuid)}
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      placement_node_uuids: toggleNodeValue(
                        current.placement_node_uuids,
                        node.uuid,
                        event.target.checked,
                      ),
                    }))
                  }
                />
                <span>{formatNodeLabel(node)}</span>
              </label>
            ))}
          </div>
        </div>
      ) : null}

      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1">
          <div className="text-sm font-medium">Placement weight override</div>
          <Input
            value={form.placement_weight_override}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                placement_weight_override: event.target.value,
              }))
            }
            placeholder="optional"
          />
        </div>
        <div className="space-y-1">
          <div className="text-sm font-medium">Placement priority override</div>
          <Input
            value={form.placement_priority_override}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                placement_priority_override: event.target.value,
              }))
            }
            placeholder="optional"
          />
        </div>
      </div>

      {isLoadingNodes ? (
        <div className="text-xs text-[hsl(var(--muted-foreground))]">Loading nodes…</div>
      ) : null}
    </div>
  )
}
