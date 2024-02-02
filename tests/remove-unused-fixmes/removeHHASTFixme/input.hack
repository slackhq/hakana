async function foo(): Awaitable<void> {
    /* HHAST_FIXME[NoJoinInAsyncFunction] */
    echo(5);
}