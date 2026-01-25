//! Project type detection and debugger recommendations
//!
//! Detects project types from the current directory and recommends appropriate debuggers.

use std::path::Path;

/// Detected project type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectType {
    Rust,
    Cuda,
    Go,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    CSharp,
    Java,
}

/// Detect project types in a directory
pub fn detect_project_types(dir: &Path) -> Vec<ProjectType> {
    let mut types = Vec::new();

    // Rust
    if dir.join("Cargo.toml").exists() {
        types.push(ProjectType::Rust);
    }

    // CUDA detection must precede C/C++ (.cu files are valid C++ but require CUDA-GDB)
    if has_extension_in_dir(dir, "cu") {
        types.push(ProjectType::Cuda);
    }

    // Go
    if dir.join("go.mod").exists() || dir.join("go.sum").exists() {
        types.push(ProjectType::Go);
    }

    // Python
    if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("requirements.txt").exists()
        || dir.join("Pipfile").exists()
    {
        types.push(ProjectType::Python);
    }

    // JavaScript / TypeScript
    if dir.join("package.json").exists() {
        // Check for TypeScript
        if dir.join("tsconfig.json").exists() {
            types.push(ProjectType::TypeScript);
        } else {
            types.push(ProjectType::JavaScript);
        }
    }

    // C / C++
    if dir.join("CMakeLists.txt").exists()
        || dir.join("Makefile").exists()
        || dir.join("configure").exists()
        || dir.join("meson.build").exists()
    {
        // Try to detect if it's C or C++
        if has_cpp_files(dir) {
            types.push(ProjectType::Cpp);
        } else if has_c_files(dir) {
            types.push(ProjectType::C);
        } else {
            // Default to C++ for CMake/Makefile projects
            types.push(ProjectType::Cpp);
        }
    }

    // C#
    if has_extension_in_dir(dir, "csproj") || has_extension_in_dir(dir, "sln") {
        types.push(ProjectType::CSharp);
    }

    // Java
    if dir.join("pom.xml").exists()
        || dir.join("build.gradle").exists()
        || dir.join("build.gradle.kts").exists()
    {
        types.push(ProjectType::Java);
    }

    types
}

/// Get recommended debuggers for a project type
pub fn debuggers_for_project(project: &ProjectType) -> Vec<&'static str> {
    match project {
        ProjectType::Rust => vec!["codelldb", "lldb"],
        ProjectType::Cuda => vec!["cuda-gdb"],
        ProjectType::Go => vec!["go"],
        ProjectType::Python => vec!["python"],
        ProjectType::JavaScript | ProjectType::TypeScript => vec!["js-debug"],
        ProjectType::C | ProjectType::Cpp => vec!["lldb", "codelldb"],
        ProjectType::CSharp => vec![], // netcoredbg not yet implemented
        ProjectType::Java => vec![],   // java-debug not yet implemented
    }
}

/// Check if directory contains C++ files
fn has_cpp_files(dir: &Path) -> bool {
    has_extension_in_dir(dir, "cpp")
        || has_extension_in_dir(dir, "cc")
        || has_extension_in_dir(dir, "cxx")
        || has_extension_in_dir(dir, "hpp")
        || has_extension_in_dir(dir, "hxx")
}

/// Check if directory contains C files
fn has_c_files(dir: &Path) -> bool {
    has_extension_in_dir(dir, "c") || has_extension_in_dir(dir, "h")
}

/// Check if directory contains files with a specific extension
fn has_extension_in_dir(dir: &Path, ext: &str) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries {
            // Explicitly handle Result - skip entries that can't be read
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().map(|e| e == ext).unwrap_or(false) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_rust_project() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        let types = detect_project_types(dir.path());
        assert!(types.contains(&ProjectType::Rust));
    }

    #[test]
    fn test_detect_python_project() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "requests").unwrap();
        let types = detect_project_types(dir.path());
        assert!(types.contains(&ProjectType::Python));
    }

    #[test]
    fn test_debuggers_for_rust() {
        let debuggers = debuggers_for_project(&ProjectType::Rust);
        assert!(debuggers.contains(&"codelldb"));
    }
}
