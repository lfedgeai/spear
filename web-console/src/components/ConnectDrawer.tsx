import { useMemo } from 'react'
import { type ExecutionSummary, type InstanceSummary, type TaskSummary } from '../api/spearApi'
import { Button } from '../../../web-admin/src/shared/components/ui/button'
import { Input } from '../../../web-admin/src/shared/components/ui/input'
import { Select } from '../../../web-admin/src/shared/components/ui/select'

export function ConnectDrawer(props: {
  tab: 'execution' | 'endpoint'
  tasks: TaskSummary[]
  instances: InstanceSummary[]
  executions: ExecutionSummary[]
  selectedTaskId: string
  selectedInstanceId: string
  selectedExecutionId: string
  endpointSearch: string
  selectedGatewayEndpoint: string
  loading: boolean
  error: string
  status: 'disconnected' | 'connecting' | 'connected'
  onChangeTab: (tab: 'execution' | 'endpoint') => void
  onOpenInfo: () => void
  onClose: () => void
  onChangeTask: (taskId: string) => void
  onChangeInstance: (instanceId: string) => void
  onChangeExecution: (executionId: string) => void
  onChangeEndpointSearch: (q: string) => void
  onSelectEndpoint: (e: { taskId: string; gatewayEndpoint: string }) => void
  onConnect: () => void | Promise<void>
}) {
  const endpointItems = useMemo(() => {
    const q = props.endpointSearch.trim().toLowerCase()
    return props.tasks
      .map((t) => ({
        taskId: t.task_id,
        taskName: t.name,
        gatewayEndpoint: (t.endpoint ?? '').trim(),
      }))
      .filter((x) => x.gatewayEndpoint.trim())
      .filter((x) => {
        if (!q) return true
        return x.gatewayEndpoint.toLowerCase().includes(q) || x.taskName.toLowerCase().includes(q)
      })
      .sort((a, b) => a.gatewayEndpoint.localeCompare(b.gatewayEndpoint))
  }, [props.endpointSearch, props.tasks])

  const targetLabel =
    props.tab === 'endpoint'
      ? props.selectedGatewayEndpoint.trim()
        ? `/e/${props.selectedGatewayEndpoint.trim()}/ws`
        : '—'
      : props.selectedExecutionId.trim()
        ? `execution: ${props.selectedExecutionId.trim()}`
        : '—'

  const canConnect =
    !props.loading &&
    props.status !== 'connecting' &&
    (props.tab === 'endpoint'
      ? !!props.selectedGatewayEndpoint.trim()
      : !!props.selectedExecutionId.trim())

  return (
    <div className="cw-drawerBackdrop" role="dialog" aria-modal="true" onClick={() => props.onClose()}>
      <div
        className="cw-drawer"
        onClick={(e) => {
          e.stopPropagation()
        }}
      >
        <div className="cw-drawerHeader">
          <div className="cw-drawerHeading">
            <div className="cw-drawerEyebrow">Connection Setup</div>
            <div className="cw-drawerTitle">Connect</div>
            <div className="cw-drawerDescription">
              Pick an execution or endpoint target, then open a live console session.
            </div>
          </div>
          <div className="cw-drawerHeaderRight">
            <Button
              variant="secondary"
              size="sm"
              onClick={props.onOpenInfo}
              disabled={props.tab === 'execution' ? !props.selectedExecutionId.trim() : false}
            >
              <span className="cw-btnGlyph">i</span>
              <span className="cw-btnText">Info</span>
            </Button>
            <Button variant="secondary" size="sm" onClick={props.onClose}>
              <span className="cw-btnGlyph">×</span>
              <span className="cw-btnText">Close</span>
            </Button>
          </div>
        </div>

        <div className="cw-drawerBody">
          <div className="cw-tabs">
            <Button
              variant={props.tab === 'execution' ? 'default' : 'secondary'}
              size="sm"
              onClick={() => props.onChangeTab('execution')}
            >
              By Execution
            </Button>
            <Button
              variant={props.tab === 'endpoint' ? 'default' : 'secondary'}
              size="sm"
              onClick={() => props.onChangeTab('endpoint')}
            >
              By Endpoint
            </Button>
          </div>

          {props.tab === 'execution' ? (
            <div className="cw-drawerSection">
              <div className="cw-sectionTitle">Execution target</div>
              <div className="cw-modalRow">
                <div className="cw-label">Task</div>
                <Select value={props.selectedTaskId} onChange={(e) => props.onChangeTask(e.target.value)}>
                  <option value="">Select a task…</option>
                  {props.tasks.map((t) => (
                    <option key={t.task_id} value={t.task_id}>
                      {t.name} ({t.task_id})
                    </option>
                  ))}
                </Select>
              </div>

              <div className="cw-modalRow">
                <div className="cw-label">Instance</div>
                <Select
                  value={props.selectedInstanceId}
                  onChange={(e) => props.onChangeInstance(e.target.value)}
                  disabled={!props.selectedTaskId}
                >
                  <option value="">Select an instance…</option>
                  {props.instances.map((i) => (
                    <option key={i.instance_id} value={i.instance_id}>
                      {i.instance_id} ({i.status})
                    </option>
                  ))}
                </Select>
              </div>

              <div className="cw-modalRow">
                <div className="cw-label">Execution</div>
                <Select
                  value={props.selectedExecutionId}
                  onChange={(e) => props.onChangeExecution(e.target.value)}
                  disabled={!props.selectedInstanceId}
                >
                  <option value="">Select an execution…</option>
                  {props.executions.map((x) => (
                    <option key={x.execution_id} value={x.execution_id}>
                      {x.execution_id} ({x.status}) {x.function_name ? `- ${x.function_name}` : ''}
                    </option>
                  ))}
                </Select>
              </div>
            </div>
          ) : (
            <div className="cw-drawerSection">
              <div className="cw-sectionTitle">Endpoint target</div>
              <div className="cw-modalRow">
                <div className="cw-label">Endpoint</div>
                <Input
                  value={props.endpointSearch}
                  onChange={(e) => props.onChangeEndpointSearch(e.target.value)}
                  placeholder="Search gateway_endpoint…"
                />
              </div>

              <div className="cw-endpointList" role="list">
                {endpointItems.length ? (
                  endpointItems.map((it) => (
                    <button
                      key={`${it.taskId}:${it.gatewayEndpoint}`}
                      className={
                        it.gatewayEndpoint === props.selectedGatewayEndpoint
                          ? 'cw-endpointItem cw-endpointItemActive'
                          : 'cw-endpointItem'
                      }
                      onClick={() =>
                        props.onSelectEndpoint({ taskId: it.taskId, gatewayEndpoint: it.gatewayEndpoint })
                      }
                      role="listitem"
                    >
                      <div className="cw-endpointLeft">
                        <div className="cw-endpointName">{it.gatewayEndpoint}</div>
                        <div className="cw-endpointMeta">{it.taskName}</div>
                      </div>
                      <div className="cw-endpointRight">
                        {it.gatewayEndpoint === props.selectedGatewayEndpoint ? 'Selected' : ''}
                      </div>
                    </button>
                  ))
                ) : (
                  <div className="cw-modalHint">No endpoints found.</div>
                )}
              </div>
            </div>
          )}

          <div className="cw-drawerSection">
            <div className="cw-sectionTitle">Connection details</div>
            <div className="cw-connDetails">
              <div className="cw-connDetailsRow">
                <div className="cw-connDetailsKey">Target</div>
                <div className="cw-connDetailsVal">{targetLabel}</div>
              </div>
              <div className="cw-connDetailsRow">
                <div className="cw-connDetailsKey">Protocol</div>
                <div className="cw-connDetailsVal">
                  {props.tab === 'endpoint' ? 'ssf.v1 (binary WS)' : 'stream session (binary WS)'}
                </div>
              </div>
            </div>
          </div>

          {props.loading ? <div className="cw-modalHint">Loading…</div> : null}
          {props.error ? <div className="cw-error">{props.error}</div> : null}
        </div>

        <div className="cw-drawerFooter">
          <Button variant="secondary" size="sm" onClick={props.onClose}>
            <span className="cw-btnGlyph">↩</span>
            <span className="cw-btnText">Cancel</span>
          </Button>
          <Button size="sm" onClick={props.onConnect} disabled={!canConnect}>
            <span className="cw-btnGlyph">↗</span>
            <span className="cw-btnText">Connect</span>
          </Button>
        </div>
      </div>
    </div>
  )
}
