class UnusedMethodClass {
    <<Hakana\MustUse>>
	public async function getEncodedId(): Awaitable<string> {
		return '';
	}

	public async function doWork(): Awaitable<string> {
		return '';
	}
}

async function foo(): Awaitable<void> {
    $c = new UnusedMethodClass();
    await $c->getEncodedId();
    await $c->doWork();
}

function foo2(): void {
    $c = new UnusedMethodClass();
    Asio\join($c->getEncodedId());
    Asio\join($c->doWork());
}