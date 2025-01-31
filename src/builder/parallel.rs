use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::time::SystemTime;

use itertools::Itertools;
use rayon::prelude::*;

use crate::builder::{FileEntry, TargetEntries};
use crate::cache::CacheValue;
use crate::progress::BuildProgressBar;
use crate::{Context, Object, ObjectID, ObjectType, RelativePath};

pub(crate) fn build(
    ctx: &Context,
    pb: &BuildProgressBar,
    target_entries: TargetEntries,
) -> io::Result<Object> {
    let max_depth = target_entries.max_depth;
    let (files, mut dirs) = target_entries
        .files
        .into_iter()
        .partition::<Vec<_>, _>(|entry| matches!(entry.mode, ObjectType::File));

    let mut objects_per_dir = files
        .into_par_iter()
        .fold(
            HashMap::new,
            |mut acc: HashMap<RelativePath, Vec<_>>, entry| {
                let parent = entry.path.parent();
                let object =
                    process_file_content(ctx, &entry).expect("failed to process file content");
                acc.entry(parent).or_default().push(object);
                pb.inc_file(1);
                acc
            },
        )
        .reduce(HashMap::new, merge_hashmap);

    for i in (1..max_depth).rev() {
        let (target, rest) = dirs
            .into_iter()
            .partition::<Vec<_>, _>(|entry| entry.depth == i);
        dirs = rest;

        let tmp = target
            .into_par_iter()
            .fold(
                HashMap::new,
                |mut acc: HashMap<RelativePath, Vec<_>>, entry| {
                    let parent = entry.path.parent();
                    if let Some(object) =
                        process_tree_content(ctx, &objects_per_dir, &entry).unwrap()
                    {
                        acc.entry(parent).or_default().push(object);
                    }
                    pb.inc_dir(1);

                    acc
                },
            )
            .reduce(HashMap::new, merge_hashmap);
        objects_per_dir = merge_hashmap(objects_per_dir, tmp);
    }

    let root = RelativePath::Root;
    let mut objects = objects_per_dir.remove(&root).unwrap();
    objects.par_sort_unstable();

    let object_id = ctx.write_tree_contents(&objects)?;
    Ok(Object::new_tree(object_id, PathBuf::from("")))
}

fn merge_hashmap<K: std::hash::Hash + Eq + Clone, V: Clone>(
    map1: HashMap<K, Vec<V>>,
    map2: HashMap<K, Vec<V>>,
) -> HashMap<K, Vec<V>> {
    map2.into_iter().fold(map1, |mut acc, (k, vs)| {
        acc.entry(k).or_default().extend(vs);
        acc
    })
}

fn process_tree_content(
    ctx: &Context,
    map: &HashMap<RelativePath, Vec<Object>>,
    entry: &FileEntry,
) -> io::Result<Option<Object>> {
    let objects = match map.get(&entry.path) {
        Some(objects) => objects.iter().sorted().collect::<Vec<_>>(),
        None => return Ok(None), // empty dir
    };

    match &entry.path {
        RelativePath::Path(path) => {
            let object_id = ctx.write_tree_contents(&objects)?;
            Ok(Some(Object::new_tree(object_id, path)))
        }
        RelativePath::Root => {
            let object_id = ctx.write_tree_contents(&objects)?;
            Ok(Some(Object::new_tree(object_id, PathBuf::from(""))))
        }
    }
}

fn process_file_content(ctx: &Context, entry: &FileEntry) -> io::Result<Object> {
    let path = ctx.root_dir().join(entry.path.as_path());
    let file_name = entry.path.file_name().ok_or(io::Error::new(
        io::ErrorKind::NotFound,
        "failed to get file_name",
    ))?;

    let metadata = fs::metadata(&path)?;
    let mtime = metadata
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .unwrap_or(std::time::Duration::new(0, 0))
        .as_micros();
    let cache_key = entry.path.as_path();

    // cache hit!
    match ctx.read_cache(&cache_key) {
        Some(cache_value) if cache_value.mtime == mtime && cache_value.size == metadata.len() => {
            return Ok(Object::new_file(cache_value.object_id, file_name));
        }
        _ => {}
    }

    let mut file = File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let object_id = ObjectID::from_contents(&contents);
    let cache_value = CacheValue {
        mtime,
        size: metadata.len(),
        object_id,
    };
    // save cache
    ctx.write_cache(&cache_key, cache_value);

    Ok(Object::new_file(object_id, file_name))
}
