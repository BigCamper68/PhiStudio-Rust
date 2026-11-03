use super::*;
use crate::Provider;

impl Host {
    pub fn provider(
        &mut self,
        plugin: &str,
        provider: &Provider,
        files: ProviderFiles,
        mut context: EditorContext,
    ) -> Result<CallResult, HostError> {
        if files.operation == 0
            || files.input.id != 1
            || files.output != 2
            || files.input.size > MAX_PROVIDER_FILE_BYTES
            || !valid_source_name(&files.input.name)
        {
            return Err(error(plugin, "Invalid provider file descriptor"));
        }
        if matches!(
            provider,
            Provider::Format {
                operation: FormatOperation::Encode,
                ..
            }
        ) && !self.granted(plugin, Capability::DocumentRead)
        {
            return Err(error(plugin, "Format export requires DocumentRead"));
        }
        self.require(plugin, |c| match (provider, c) {
            (
                Provider::Format { id, operation },
                Contribution::Format {
                    id: registered,
                    decode,
                    encode,
                    ..
                },
            ) => {
                id == registered
                    && match operation {
                        FormatOperation::Decode => *decode,
                        FormatOperation::Encode => *encode,
                    }
            }
            (
                Provider::Resource { id, resource },
                Contribution::Resource {
                    id: registered,
                    resource: kind,
                    ..
                },
            ) => id == registered && resource == kind,
            _ => false,
        })?;
        let bridge = self.slots[plugin]
            .contributions
            .iter()
            .find_map(|c| match c {
                Contribution::Format { id, bridge, .. } if id == provider.id() => Some(*bridge),
                _ => None,
            })
            .unwrap_or_default();
        if bridge == FormatBridge::Project
            && matches!(
                provider,
                Provider::Format {
                    operation: FormatOperation::Encode,
                    ..
                }
            )
            && !self.granted(plugin, Capability::ProjectRead)
        {
            return Err(error(plugin, "Project format export requires ProjectRead"));
        }
        context.chart = None;
        context.selection = Value::Null;
        let request = match provider {
            Provider::Format { id, operation } => Request::Format {
                id: id.clone(),
                operation: *operation,
                bridge,
                files,
                context,
            },
            Provider::Resource { id, resource } => Request::Resource {
                id: id.clone(),
                resource: *resource,
                files,
                context,
            },
        };
        let response = self.call(plugin, request)?;
        Ok(CallResult {
            effects: owned(&self.slots[plugin], response.effects),
            result: response.result,
        })
    }
}
