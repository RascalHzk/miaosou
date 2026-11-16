//! tag 筛选集成测试的共享辅助（两个测试文件共用，避免各拷一份）。

#![allow(dead_code)] // 每个测试二进制只用到其中一部分

use plist::{Value, to_writer_binary};
use search_cache::{SearchOutcome, SlabIndex};
use std::path::Path;
use xattr::set;

pub const USER_TAG_XATTR: &str = "com.apple.metadata:_kMDItemUserTags";

pub fn guard_indices(result: Result<SearchOutcome, anyhow::Error>) -> Vec<SlabIndex> {
    {
        result
            .expect("search should succeed")
            .nodes
            .expect("noop token should not cancel")
    }
}

pub fn write_tags(path: &Path, tags: &[&str]) {
    {
        let values: Vec<Value> = tags
            .iter()
            .map(|tag| Value::String(format!("{tag}\n0")))
            .collect();
        let mut data = Vec::new();
        to_writer_binary(&mut data, &Value::Array(values)).expect("serialize tags");
        set(path, USER_TAG_XATTR, &data).expect("write tag xattr");
    }
}
