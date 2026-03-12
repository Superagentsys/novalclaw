
pub fn import_skills_from_dir(source_dir: &Path, target_dir: &Path, overwrite: bool) -> Result<usize> {
    if !source_dir.exists() {
        anyhow::bail!("Source directory does not exist: {:?}", source_dir);
    }
    if !target_dir.exists() {
        fs::create_dir_all(target_dir)?;
    }

    let mut count = 0;
    // Iterate over subdirectories in source_dir
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let skill_file = path.join("SKILL.md");
            if skill_file.exists() {
                let skill_name = path.file_name().unwrap();
                let target_skill_dir = target_dir.join(skill_name);

                if target_skill_dir.exists() {
                    if !overwrite {
                        // Skipping
                        continue;
                    }
                } else {
                    fs::create_dir_all(&target_skill_dir)?;
                }

                // Copy all files
                for sub_entry in fs::read_dir(&path)? {
                    let sub_entry = sub_entry?;
                    let sub_path = sub_entry.path();
                    if sub_path.is_file() {
                        fs::copy(&sub_path, target_skill_dir.join(sub_entry.file_name()))?;
                    } else if sub_path.is_dir() {
                        // Simple recursive copy for 1 level deep (e.g. scripts/)
                        let sub_dir_name = sub_entry.file_name();
                        let target_sub_dir = target_skill_dir.join(sub_dir_name);
                        fs::create_dir_all(&target_sub_dir)?;
                        for deep_entry in fs::read_dir(&sub_path)? {
                            let deep_entry = deep_entry?;
                            if deep_entry.path().is_file() {
                                fs::copy(deep_entry.path(), target_sub_dir.join(deep_entry.file_name()))?;
                            }
                        }
                    }
                }
                count += 1;
            }
        }
    }
    Ok(count)
}
