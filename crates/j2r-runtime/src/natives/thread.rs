use super::*;

pub(crate) fn n_thread_init_v_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_init_v(recv, args)
}
pub(crate) fn n_thread_init_r_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_init_r(recv, args)
}
pub(crate) fn n_thread_init_n_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_init_n(recv, args)
}
pub(crate) fn n_thread_init_rn_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_init_rn(recv, args)
}
pub(crate) fn n_thread_start_w(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_start(recv, ctx)
}
pub(crate) fn n_thread_run_w(
    recv: Option<&JRef>,
    _args: &[Value],
    ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_run(recv, ctx)
}
pub(crate) fn n_thread_join0_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_join(recv, &[])
}
pub(crate) fn n_thread_join1_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_join(recv, args)
}
pub(crate) fn n_thread_join2_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_join(recv, args)
}
pub(crate) fn n_thread_sleep1_w(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_sleep(args)
}
pub(crate) fn n_thread_sleep2_w(
    _recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_sleep(args)
}
pub(crate) fn n_thread_yield_w(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_yield()
}
pub(crate) fn n_thread_current_thread_w(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_current_thread()
}
pub(crate) fn n_thread_set_daemon_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_set_daemon(recv, args)
}
pub(crate) fn n_thread_is_daemon_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_is_daemon(recv)
}
pub(crate) fn n_thread_get_name_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_get_name(recv)
}
pub(crate) fn n_thread_set_name_w(
    recv: Option<&JRef>,
    args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_set_name(recv, args)
}
pub(crate) fn n_thread_is_alive_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_is_alive(recv)
}
pub(crate) fn n_thread_active_count_w(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_active_count()
}
pub(crate) fn n_thread_interrupt_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_interrupt(recv)
}
pub(crate) fn n_thread_is_interrupted_w(
    recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_is_interrupted(recv)
}
pub(crate) fn n_thread_interrupted_w(
    _recv: Option<&JRef>,
    _args: &[Value],
    _ctx: Dispatch,
) -> Result<Value, Throw> {
    crate::thread::n_thread_interrupted()
}
