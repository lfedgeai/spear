import type { Dispatch, SetStateAction } from 'react'

import { Input } from '@/components/ui/input'

import type { AiBackendEditorFormState } from './AiBackendEditorForm'
import CredentialRefPicker from './CredentialRefPicker'

export default function RemoteBackendFields(props: {
  form: AiBackendEditorFormState
  setForm: Dispatch<SetStateAction<AiBackendEditorFormState>>
}) {
  return (
    <>
      <div className="space-y-1">
        <div className="text-sm font-medium">Base URL</div>
        <Input
          value={props.form.base_url}
          onChange={(event) =>
            props.setForm((current) => ({ ...current, base_url: event.target.value }))
          }
          placeholder="e.g. https://api.openai.com/v1"
        />
      </div>

      <div className="space-y-1">
        <div className="text-sm font-medium">Credential ref</div>
        <CredentialRefPicker
          value={props.form.credential_ref}
          providerKind={props.form.provider}
          onChange={(value) =>
            props.setForm((current) => ({ ...current, credential_ref: value }))
          }
        />
      </div>
    </>
  )
}
