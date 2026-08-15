use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A portable, platform-independent path representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PortablePath {
    components: Vec<String>,
    is_absolute: bool,
}

impl PortablePath {
    pub fn new(path: &str) -> Self {
        let normalized = path.replace('\\', "/");
        let is_absolute = normalized.starts_with('/') || normalized.contains(':');
        
        let components: Vec<String> = normalized
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .map(String::from)
            .collect();
        
        Self {
            components,
            is_absolute,
        }
    }
    
    pub fn from_pathbuf(path: &PathBuf) -> Self {
        Self::new(&path.to_string_lossy())
    }
    
    pub fn to_native(&self) -> PathBuf {
        if self.components.is_empty() {
            return PathBuf::from(".");
        }
        
        let mut path = PathBuf::new();
        
        if self.is_absolute {
            #[cfg(unix)]
            {
                path.push("/");
            }
        }
        
        for component in &self.components {
            path.push(component);
        }
        
        path
    }
    
    pub fn to_portable_string(&self) -> String {
        let mut result = String::new();
        
        if self.is_absolute {
            result.push('/');
        }
        
        result.push_str(&self.components.join("/"));
        result
    }
    
    pub fn join(&self, other: &str) -> Self {
        let mut new_components = self.components.clone();
        
        for component in other.split('/') {
            if component == ".." && !new_components.is_empty() {
                new_components.pop();
            } else if !component.is_empty() && component != "." {
                new_components.push(component.to_string());
            }
        }
        
        Self {
            components: new_components,
            is_absolute: self.is_absolute,
        }
    }
    
    pub fn parent(&self) -> Option<Self> {
        if self.components.is_empty() {
            return None;
        }
        
        let mut new_components = self.components.clone();
        new_components.pop();
        
        Some(Self {
            components: new_components,
            is_absolute: self.is_absolute,
        })
    }
    
    pub fn file_name(&self) -> Option<&str> {
        self.components.last().map(|s| s.as_str())
    }
    
    pub fn is_absolute(&self) -> bool {
        self.is_absolute
    }
    
    pub fn is_relative(&self) -> bool {
        !self.is_absolute
    }
    
    pub fn starts_with(&self, prefix: &PortablePath) -> bool {
        if self.is_absolute != prefix.is_absolute {
            return false;
        }
        
        if prefix.components.len() > self.components.len() {
            return false;
        }
        
        self.components
            .iter()
            .zip(prefix.components.iter())
            .all(|(a, b)| a == b)
    }
}

impl From<&str> for PortablePath {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<PathBuf> for PortablePath {
    fn from(p: PathBuf) -> Self {
        Self::from_pathbuf(&p)
    }
}

impl std::fmt::Display for PortablePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_portable_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_portable_path_creation() {
        let path = PortablePath::new("/home/user/file.txt");
        assert!(path.is_absolute());
        assert_eq!(path.components.len(), 3);
    }
    
    #[test]
    fn test_path_normalization() {
        let path1 = PortablePath::new("C:\\Users\\test\\file.txt");
        let path2 = PortablePath::new("C:/Users/test/file.txt");
        
        // Both should normalize to the same portable format
        assert_eq!(path1.to_portable_string(), path2.to_portable_string());
    }
    
    #[test]
    fn test_path_join() {
        let base = PortablePath::new("/home/user");
        let joined = base.join("documents/file.txt");
        
        assert_eq!(joined.to_portable_string(), "/home/user/documents/file.txt");
    }
    
    #[test]
    fn test_parent() {
        let path = PortablePath::new("/home/user/file.txt");
        let parent = path.parent().unwrap();
        
        assert_eq!(parent.to_portable_string(), "/home/user");
    }
    
    #[test]
    fn test_file_name() {
        let path = PortablePath::new("/home/user/file.txt");
        assert_eq!(path.file_name(), Some("file.txt"));
    }
}
