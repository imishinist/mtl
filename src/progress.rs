use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

#[derive(Clone)]
pub struct BuildProgressBar {
    pb_file: Option<ProgressBar>,
    pb_dir: Option<ProgressBar>,
}

impl BuildProgressBar {
    pub fn new(num_files: u64, num_dirs: u64, enabled: bool) -> Self {
        if !enabled {
            return BuildProgressBar {
                pb_file: None,
                pb_dir: None,
            };
        }

        let style = ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:50.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .unwrap()
        .progress_chars("##-");

        let m = MultiProgress::new();
        let pb_file = {
            let pb = m.add(ProgressBar::new(num_files));
            pb.set_style(style.clone());
            pb.set_message("files");
            pb
        };
        let pb_dir = {
            let pb = m.add(ProgressBar::new(num_dirs));
            pb.set_style(style.clone());
            pb.set_message("dirs");
            pb
        };

        BuildProgressBar {
            pb_file: Some(pb_file),
            pb_dir: Some(pb_dir),
        }
    }

    pub fn finish(&self) {
        if let Some(ref pb_file) = self.pb_file {
            pb_file.finish();
        }
        if let Some(ref pb_dir) = self.pb_dir {
            pb_dir.finish();
        }
    }

    pub fn inc_file(&self, delta: u64) {
        if let Some(ref pb_file) = self.pb_file {
            pb_file.inc(delta);
        }
    }

    pub fn inc_dir(&self, delta: u64) {
        if let Some(ref pb_dir) = self.pb_dir {
            pb_dir.inc(delta);
        }
    }
}

impl Drop for BuildProgressBar {
    fn drop(&mut self) {
        self.finish();
    }
}
