import { useMemo } from 'react'
import { type ExecutionSummary, type InstanceSummary, type TaskSummary } from '../api/spearApi'

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
          <div className="cw-drawerTitle">Connect</div>
          <div className="cw-drawerHeaderRight">
            <button
              className="cw-iconBtn"
              onClick={props.onOpenInfo}
              disabled={props.tab === 'execution' ? !props.selectedExecutionId.trim() : false}
            >
              <span className="cw-btnGlyph">i</span>
              <span className="cw-btnText">Info</span>
            </button>
            <button className="cw-iconBtn" onClick={props.onClose}>
              <span className="cw-btnGlyph">×</span>
              <span className="cw-btnText">Close</span>
            </button>
          </div>
        </div>

        <div className="cw-drawerBody">
          <div className="cw-tabs">
            <button
              className={props.tab === 'execution' ? 'cw-tab cw-tabActive' : 'cw-tab'}
              onClick={() => props.onChangeTab('execution')}
            >
              By Execution
            </button>
            <button
              className={props.tab === 'endpoint' ? 'cw-tab cw-tabActive' : 'cw-tab'}
              onClick={() => props.onChangeTab('endpoint')}
            >
              By Endpoint
            </button>
          </div>

          {props.tab === 'execution' ? (
            <div className="cw-drawerSection">
              <div className="cw-modalRow">
                <div className="cw-label">Task</div>
                <select className="cw-select" value={props.selectedTaskId} onChange={(e) => props.onChangeTask(e.target.value)}>
                  <option value="">Select a task…</option>
                  {props.tasks.map((t) => (
                    <option key={t.task_id} value={t.task_id}>
                      {t.name} ({t.task_id})
                    </option>
                  ))}
                </select>
              </div>

              <div className="cw-modalRow">
                <div className="cw-label">Instance</div>
                <select
                  className="cw-select"
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
                </select>
              </div>

              <div className="cw-modalRow">
                <div className="cw-label">Execution</div>
                <select
                  className="cw-select"
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
                </select>
              </div>
            </div>
          ) : (
            <div className="cw-drawerSection">
              <div className="cw-modalRow">
                <div className="cw-label">Endpoint</div>
                <input
                  className="cw-textInput"
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
            <div className="cw-label">Connection details</div>
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
          <button className="cw-btn" onClick={props.onClose}>
            <span className="cw-btnGlyph">↩</span>
            <span className="cw-btnText">Cancel</span>
          </button>
          <button className="cw-btn cw-btnPrimary" onClick={props.onConnect} disabled={!canConnect}>
            <span className="cw-btnGlyph">↗</span>
            <span className="cw-btnText">Connect</span>
          </button>
        </div>
      </div>
    </div>
  )
}
