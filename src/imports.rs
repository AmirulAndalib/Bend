use crate::{
  diagnostics::{Diagnostics, DiagnosticsConfig},
  fun::{load_book::do_parse_book, Book, Ctx, Name, Source},
};
use std::{
  collections::{hash_map::Entry, HashMap, HashSet},
  path::PathBuf,
};

#[derive(Debug, Clone, Default)]
pub struct Imports {
  /// Imports declared in the program source.
  names: Vec<(Name, Vec<Name>)>,

  /// Map from binded names to source package.
  map: HashMap<Name, Name>,

  /// Imported packages to be loaded in the program.
  /// When loaded, the book contents are drained to the parent book,
  /// adjusting def names and refs accordingly.
  pkgs: Vec<(Name, Book)>,
}

impl Imports {
  pub fn add_import(&mut self, import: Name, sub_imports: Vec<Name>) -> Result<(), Name> {
    if import.contains('@') && !import.contains('/') {
      return Err(import);
    }

    self.names.push((import, sub_imports));
    Ok(())
  }

  pub fn imports(&self) -> &[(Name, Vec<Name>)] {
    &self.names
  }

  pub fn load_imports(&mut self, loader: &mut impl PackageLoader) -> Result<(), String> {
    for (src, sub_imports) in &self.names {
      let packages = loader.load_multiple(src.clone(), sub_imports)?;

      for (psrc, code) in packages {
        let mut module = do_parse_book(&code, &psrc, Book::default())?;
        module.imports.load_imports(loader)?;
        self.pkgs.push((psrc, module));
      }

      let (_namespace, name) = src.split_once('/').unwrap();

      if let Entry::Vacant(v) = self.map.entry(Name::new(name)) {
        v.insert(src.clone());
      }

      for sub in sub_imports {
        if let Entry::Vacant(v) = self.map.entry(sub.clone()) {
          v.insert(Name::new(format!("{}/{}", src, sub)));
        }
      }
    }

    Ok(())
  }
}

impl Book {
  pub fn apply_imports(&mut self, diagnostics_cfg: DiagnosticsConfig) -> Result<(), Diagnostics> {
    // TODO: Check for missing imports from local files
    // TODO: handle adts and ctrs
    for (src, package) in &mut self.imports.pkgs {
      package.apply_imports(diagnostics_cfg)?;

      let mut ctx = Ctx::new(package, diagnostics_cfg);
      ctx.resolve_refs()?; // TODO: does not work for adts

      let mut defs = std::mem::take(&mut package.defs);

      for def in defs.values_mut() {
        match def.source {
          Source::Normal(..) if self.imports.map.contains_key(&def.name) => {
            def.source = Source::Imported;
          }
          Source::Builtin => {}
          Source::Generated => {}
          Source::Inaccessible => {}
          _ => {
            def.source = Source::Inaccessible;

            // Mangle inaccessible definitions so that users cant call them
            let new_name = if let Some(n) = package.imports.map.get(&def.name) {
              Name::new(format!("__{}__", n))
            } else {
              Name::new(format!("__{}/{}__", src, def.name))
            };

            package.imports.map.insert(def.name.clone(), new_name.clone());
            def.name = new_name;
          }
        }
      }

      for (_, mut def) in defs {
        def.subst_refs(&package.imports.map);
        self.defs.insert(def.name.clone(), def);
      }
    }

    Ok(())
  }
}

pub trait PackageLoader {
  /// Loads a package.
  /// Should only return `Ok(None)` if the package is already loaded
  fn load(&mut self, name: Name) -> Result<Option<(Name, String)>, String>;
  fn load_multiple(&mut self, name: Name, sub_names: &[Name]) -> Result<Vec<(Name, String)>, String>;
  fn is_loaded(&self, name: &Name) -> bool;
}

pub struct DefaultLoader<T: Fn(&str) -> Result<String, String>> {
  pub local_path: Option<PathBuf>,
  pub loaded: HashSet<Name>,
  pub load_fn: T,
}

impl<T: Fn(&str) -> Result<String, String>> PackageLoader for DefaultLoader<T> {
  fn load(&mut self, name: Name) -> Result<Option<(Name, String)>, String> {
    if !self.is_loaded(&name) {
      self.loaded.insert(name.clone());
      (self.load_fn)(&name).map(|pack| Some((name, pack)))
    } else {
      Ok(None)
    }
  }

  fn load_multiple(&mut self, name: Name, sub_names: &[Name]) -> Result<Vec<(Name, String)>, String> {
    if name.contains('@') {
      let mut packages = Vec::new();

      if !sub_names.is_empty() {
        for sub in sub_names {
          if let Some(p) = self.load(Name::new(&(format!("{}/{}", name, sub))))? {
            packages.push(p);
          }
        }
      }

      if let Some(package) = self.load(name)? {
        packages.push(package)
      }

      Ok(packages)
    } else if let Some(path) = &self.local_path {
      // Loading local packages is different than non-local ones,
      // sub_names refer to top level definitions on the imported file.
      // This should match the behaviour of importing a uploaded version of the imported file,
      // as each def will be saved separately.

      // TODO: Should the local filesystem be searched anyway for each sub_name?

      let path = path.parent().unwrap().join(name.as_ref()).with_extension("bend");
      std::fs::read_to_string(path).map_err(|e| e.to_string()).map(|c| vec![(name, c)])
    } else {
      Err(format!(
        "Can not import local '{}'. Use 'version@{}' if you wish to import a online package.",
        name, name
      ))
    }
  }

  fn is_loaded(&self, name: &Name) -> bool {
    self.loaded.contains(name)
  }
}

#[allow(clippy::field_reassign_with_default)]
/// Check book without warnings about unused definitions
pub fn check_book(book: &mut Book) -> Result<Diagnostics, Diagnostics> {
  let mut diagnostics_cfg = DiagnosticsConfig::default();
  diagnostics_cfg.unused_definition = crate::diagnostics::Severity::Allow;
  let compile_opts = crate::CompileOpts::default();

  crate::check_book(book, diagnostics_cfg, compile_opts)
}
