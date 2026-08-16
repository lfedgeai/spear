/**
 * Placement panel for unified AI backend detail pages.
 * 统一 AI backend 详情页的 placement 面板。
 */

import { Link } from 'react-router-dom'
import { useState } from 'react'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'

import {
  deleteAiBackendPlacement,
  upsertAiBackendPlacement,
  type AiBackendPlacement,
  type WriteAiBackendPlacementInput,
} from '@/api/ai-backends'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'

import PlacementEditorDialog from './PlacementEditorDialog'

function PlacementStateBadge(props: { state: string }) {
  if (props.state === 'enabled') return <Badge variant="success">enabled</Badge>
  if (props.state === 'disabled') return <Badge variant="secondary">disabled</Badge>
  return <Badge variant="secondary">{props.state}</Badge>
}

export default function PlacementPanel(props: {
  backendId: string
  placements: AiBackendPlacement[]
  onRefetch: () => Promise<unknown>
}) {
  const queryClient = useQueryClient()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingPlacement, setEditingPlacement] = useState<AiBackendPlacement | null>(null)

  const placementMutation = useMutation({
    mutationFn: async (input: WriteAiBackendPlacementInput) => {
      const response = await upsertAiBackendPlacement(input)
      if (!response.success) {
        throw new Error(response.message || 'Failed to save placement')
      }
      return response
    },
    onSuccess: async () => {
      toast.success('Placement saved')
      await props.onRefetch()
      await queryClient.invalidateQueries({ queryKey: ['ai-backends'] })
    },
  })

  const deleteMutation = useMutation({
    mutationFn: async (placementId: string) => {
      const response = await deleteAiBackendPlacement(placementId)
      if (!response.success) {
        throw new Error(response.message || 'Failed to delete placement')
      }
      return response
    },
    onSuccess: async () => {
      toast.success('Placement deleted')
      await props.onRefetch()
      await queryClient.invalidateQueries({ queryKey: ['ai-backends'] })
    },
  })

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <CardTitle>Placements</CardTitle>
        <Button
          size="sm"
          onClick={() => {
            setEditingPlacement(null)
            setDialogOpen(true)
          }}
        >
          Add Placement
        </Button>
      </CardHeader>
      <CardContent>
        {props.placements.length > 0 ? (
          <div className="overflow-auto rounded-[var(--radius)] border border-[hsl(var(--border))]">
            <table className="w-full text-sm">
              <thead className="bg-[hsl(var(--muted))] text-left text-xs text-[hsl(var(--muted-foreground))]">
                <tr>
                  <th className="px-3 py-2">Node</th>
                  <th className="px-3 py-2">State</th>
                  <th className="px-3 py-2">Weight</th>
                  <th className="px-3 py-2">Priority</th>
                  <th className="px-3 py-2">Placement ID</th>
                  <th className="px-3 py-2">Actions</th>
                </tr>
              </thead>
              <tbody>
                {props.placements.map((placement) => (
                  <tr
                    key={placement.placement_id}
                    className="border-t border-[hsl(var(--border))]"
                  >
                    <td className="px-3 py-2 font-mono text-xs">
                      <Link
                        to={`/nodes/${encodeURIComponent(placement.node_uuid)}`}
                        className="hover:underline"
                      >
                        {placement.node_uuid}
                      </Link>
                    </td>
                    <td className="px-3 py-2">
                      <PlacementStateBadge state={placement.desired_state} />
                    </td>
                    <td className="px-3 py-2 text-xs">{placement.weight_override ?? '-'}</td>
                    <td className="px-3 py-2 text-xs">{placement.priority_override ?? '-'}</td>
                    <td className="px-3 py-2 font-mono text-xs">{placement.placement_id}</td>
                    <td className="px-3 py-2">
                      <div className="flex items-center gap-2">
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => {
                            setEditingPlacement(placement)
                            setDialogOpen(true)
                          }}
                        >
                          Edit
                        </Button>
                        <Button
                          variant="destructive"
                          size="sm"
                          onClick={async () => {
                            if (!window.confirm(`Delete placement ${placement.placement_id}?`)) {
                              return
                            }
                            await deleteMutation.mutateAsync(placement.placement_id)
                          }}
                        >
                          Delete
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="text-sm text-[hsl(var(--muted-foreground))]">No placements.</div>
        )}
      </CardContent>

      <PlacementEditorDialog
        open={dialogOpen}
        backendId={props.backendId}
        placement={editingPlacement}
        onOpenChange={setDialogOpen}
        onSubmit={async (input) => {
          await placementMutation.mutateAsync(input)
        }}
      />
    </Card>
  )
}
