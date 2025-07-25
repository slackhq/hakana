final class UnusedMethodClass {
    <<Hakana\MustUse>>
	public async function getEncodedId(): Awaitable<string> {
        await \HH\Asio\usleep(100000);
		return '';
	}

	public async function doWork(): Awaitable<string> {
        await \HH\Asio\usleep(100000);
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