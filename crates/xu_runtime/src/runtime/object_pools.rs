//! 对象池子模块
//!
//! 管理各种对象池，用于减少内存分配。

use crate::core::Env;
use crate::core::Value;
use crate::vm::{IterState, Handler};

/// 小列表池的最大容量
const SMALL_LIST_CAP: usize = 8;
/// 小列表池的最大数量
#[allow(dead_code)]
const SMALL_LIST_POOL_MAX: usize = 64;

/// 对象池管理器
///
/// 包含所有对象池相关的字段：
/// - env_pool: 环境对象池
/// - vm_stack_pool: VM 栈池
/// - vm_iters_pool: 迭代器状态池
/// - vm_handlers_pool: 异常处理器池
/// - builder_pool: 字符串构建器池
/// - small_list_pool: 小列表池 (容量 ≤8)
pub struct ObjectPools {
    /// 环境对象池
    pub env_pool: Vec<Env>,
    /// VM 栈池
    pub vm_stack_pool: Vec<Vec<Value>>,
    /// 迭代器状态池
    pub vm_iters_pool: Vec<Vec<IterState>>,
    /// 异常处理器池
    pub vm_handlers_pool: Vec<Vec<Handler>>,
    /// 字符串构建器池
    pub builder_pool: Vec<String>,
    /// 小列表池 (容量 ≤8 的列表)
    pub small_list_pool: Vec<Vec<Value>>,
}

impl ObjectPools {
    /// 创建新的对象池管理器
    pub fn new() -> Self {
        Self {
            env_pool: Vec::new(),
            vm_stack_pool: Vec::new(),
            vm_iters_pool: Vec::new(),
            vm_handlers_pool: Vec::new(),
            builder_pool: Vec::new(),
            small_list_pool: Vec::new(),
        }
    }

    /// 从环境池获取或创建新环境
    #[inline]
    #[allow(dead_code)]
    pub fn get_env(&mut self) -> Env {
        self.env_pool.pop().unwrap_or_else(Env::new)
    }

    /// 将环境返回到池中
    #[inline]
    #[allow(dead_code)]
    pub fn return_env(&mut self, env: Env) {
        self.env_pool.push(env);
    }

    /// 从栈池获取或创建新栈
    #[inline]
    pub fn get_stack(&mut self) -> Vec<Value> {
        self.vm_stack_pool.pop().unwrap_or_else(Vec::new)
    }

    /// 将栈返回到池中
    #[inline]
    pub fn return_stack(&mut self, mut stack: Vec<Value>) {
        stack.clear();
        if stack.capacity() <= 1024 {
            self.vm_stack_pool.push(stack);
        }
    }

    /// 从迭代器池获取或创建新迭代器向量
    #[inline]
    pub fn get_iters(&mut self) -> Vec<IterState> {
        self.vm_iters_pool.pop().unwrap_or_else(Vec::new)
    }

    /// 将迭代器向量返回到池中
    #[inline]
    pub fn return_iters(&mut self, mut iters: Vec<IterState>) {
        iters.clear();
        self.vm_iters_pool.push(iters);
    }

    /// 从处理器池获取或创建新处理器向量
    #[inline]
    pub fn get_handlers(&mut self) -> Vec<Handler> {
        self.vm_handlers_pool.pop().unwrap_or_else(Vec::new)
    }

    /// 将处理器向量返回到池中
    #[inline]
    pub fn return_handlers(&mut self, mut handlers: Vec<Handler>) {
        handlers.clear();
        self.vm_handlers_pool.push(handlers);
    }

    /// 从构建器池获取或创建新字符串构建器
    #[inline]
    pub fn get_builder(&mut self, cap: usize) -> String {
        if let Some(mut s) = self.builder_pool.pop() {
            s.clear();
            if s.capacity() < cap {
                s.reserve(cap - s.capacity());
            }
            s
        } else {
            String::with_capacity(cap)
        }
    }

    /// 将字符串构建器返回到池中
    #[inline]
    pub fn return_builder(&mut self, s: String) {
        // 只保留合理容量的字符串以避免内存膨胀
        if s.capacity() <= 4096 && self.builder_pool.len() < 16 {
            self.builder_pool.push(s);
        }
    }

    /// 从小列表池获取或创建新列表
    /// 仅用于容量 ≤8 的列表
    #[inline]
    pub fn get_small_list(&mut self, cap: usize) -> Option<Vec<Value>> {
        if cap <= SMALL_LIST_CAP {
            self.small_list_pool.pop().map(|mut v| {
                v.clear();
                v
            })
        } else {
            None
        }
    }

    /// 将小列表返回到池中
    #[inline]
    #[allow(dead_code)]
    pub fn return_small_list(&mut self, list: Vec<Value>) {
        if list.capacity() <= SMALL_LIST_CAP
            && self.small_list_pool.len() < SMALL_LIST_POOL_MAX
        {
            self.small_list_pool.push(list);
        }
    }
}

impl Default for ObjectPools {
    fn default() -> Self {
        Self::new()
    }
}
