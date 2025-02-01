use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::builder::TargetEntries;
use crate::progress::BuildProgressBar;
use crate::{builder, Context, Object, ObjectType, RelativePath};

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
                    builder::hash_file_entry(ctx, &entry).expect("failed to process file content");
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
                        builder::process_tree_content(ctx, &objects_per_dir, &entry).unwrap()
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
