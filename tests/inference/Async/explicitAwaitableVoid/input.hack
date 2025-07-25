function takesAwaitableVoidFunction(
    (function(): Awaitable<void>) $fn
): void {}

takesAwaitableVoidFunction(
    async (): Awaitable<void> ==> {
        await \HH\Asio\usleep(100000);
        echo "foo";
    }
);

takesAwaitableVoidFunction(
    async () ==> {
        await \HH\Asio\usleep(100000);
        echo "foo";
    }
);

function returnsVoid(): void {}

// this is allowed by the Hack typechecker and we will allow it too
takesAwaitableVoidFunction(
    async (): Awaitable<void> ==> {
        await \HH\Asio\usleep(100000);
        return returnsVoid();
    }
);

// this is allowed by the Hack typechecker and we will allow it too
takesAwaitableVoidFunction(
    async () ==> {
        await \HH\Asio\usleep(100000);
        return returnsVoid();
    }
);