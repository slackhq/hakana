async function fetch_async(): Awaitable<bool> {
    return true;
}

function fetch(): bool {
    return \HH\Asio\join(fetch_async());
}

async function run(bool $flag, ?bool $nullable): Awaitable<void> {
    echo $flag && fetch();
    echo $flag || fetch();
    echo $flag ? fetch() : fetch();
    echo $nullable ?? fetch();
    echo fetch();
    if ($flag) {
        echo fetch();
    }
}

function run_sync(bool $flag): void {
    echo $flag && fetch();
}
