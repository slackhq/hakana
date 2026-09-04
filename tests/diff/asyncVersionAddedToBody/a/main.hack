async function main_async(): Awaitable<int> {
    $s = new Svc();
    return $s->fetch();
}

<<__EntryPoint>>
function main(): void {
    Asio\join(main_async());
}
