use super::*;

pub(super) fn resource_path(reference: &str) -> Result<(), ChartError> {
    let normalized = reference.replace('\\', "/");
    let reference = normalized.as_str();
    if reference.is_empty()
        || reference.contains([':', '\0'])
        || Path::new(reference)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(invalid("Shader path must stay inside the project"));
    }
    Ok(())
}

pub fn validate_shader_source(path: &str, source: &str) -> Result<(), ChartError> {
    resource_path(path)?;
    if !path.ends_with(".glsl")
        || path.contains('\\')
        || source.is_empty()
        || source.len() > MAX_SOURCE_BYTES
        || source.contains('\0')
    {
        return Err(invalid("Use a UTF-8 .glsl file up to 1 MiB"));
    }
    Ok(())
}

pub fn shader_sources(doc: &ChartDocument) -> Result<Map<String, Value>, ChartError> {
    let Some(value) = doc.root.get(SOURCES_KEY) else {
        return Ok(Map::new());
    };
    let map = value
        .as_object()
        .ok_or_else(|| invalid("Shader sources must be an object"))?;
    if map.len() > 128 || serde_json::to_vec(map)?.len() > 8 * MAX_SOURCE_BYTES {
        return Err(invalid("Shader sources exceed the project limit"));
    }
    for (path, source) in map {
        validate_shader_source(
            path,
            source
                .as_str()
                .ok_or_else(|| invalid("Shader source must be text"))?,
        )?;
    }
    Ok(map.clone())
}

pub fn read_shader_source(
    doc: &ChartDocument,
    root: Option<&Path>,
    path: &str,
) -> Result<String, ChartError> {
    resource_path(path)?;
    if let Some(source) = shader_sources(doc)?.get(path).and_then(Value::as_str) {
        return Ok(source.into());
    }
    let root = root
        .ok_or_else(|| invalid("Open a project to load its shader files"))?
        .canonicalize()?;
    let file = root.join(path.replace('\\', "/")).canonicalize()?;
    if !file.starts_with(&root) {
        return Err(invalid("Shader path leaves the project"));
    }
    let mut source = String::new();
    fs::File::open(file)?
        .take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_string(&mut source)?;
    validate_shader_source(&path.replace('\\', "/"), &source)?;
    Ok(source)
}

/// Phira's editable uniform types and %default% annotations; built-ins stay automatic.
pub fn shader_parameters(source: &str) -> Vec<ShaderParameter> {
    let mut result = Vec::new();
    let mut block = false;
    for line in source.lines() {
        let mut code = String::new();
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if block {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    block = false;
                }
            } else if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                block = true;
            } else {
                code.push(c);
            }
        }
        let (decl, comment) = code.split_once("//").unwrap_or((&code, ""));
        for decl in decl.split(';') {
            let words: Vec<_> = decl.split_whitespace().collect();
            let Some(i) = words.iter().position(|v| *v == "uniform") else {
                continue;
            };
            let mut fields = &words[i + 1..];
            if fields
                .first()
                .is_some_and(|v| ["lowp", "mediump", "highp"].contains(v))
            {
                fields = &fields[1..];
            }
            if fields.len() != 2 {
                continue;
            }
            let (kind, name) = (fields[0], fields[1]);
            if ["time", "screenSize", "UVScale"].contains(&name)
                || !["float", "vec2", "vec4"].contains(&kind)
                || !uniform_name(name)
                || result.iter().any(|p: &ShaderParameter| p.name == name)
            {
                continue;
            }
            let values: Vec<f64> = comment
                .split('%')
                .nth(1)
                .unwrap_or("")
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            let at = |i: usize| {
                values
                    .get(i)
                    .copied()
                    .filter(|v| v.is_finite())
                    .unwrap_or(0.)
            };
            let default = match kind {
                "float" => json!(at(0)),
                "vec2" => json!([at(0), at(1)]),
                _ => json!(
                    (0..4)
                        .map(|i| (at(i).clamp(0., 1.) * 255.).round() as u8)
                        .collect::<Vec<_>>()
                ),
            };
            result.push(ShaderParameter {
                name: name.into(),
                default,
            });
        }
    }
    result
}

pub(super) fn uniform_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
