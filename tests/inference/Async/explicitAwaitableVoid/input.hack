function takesAwaitableVoidFunction(
    (function(): Awaitable<void>) $fn
): void {}

takesAwaitableVoidFunction(
    async (): Awaitable<void> ==> {
        echo "foo";
    }
);

takesAwaitableVoidFunction(
    async () ==> {
        echo "foo";
    }
);

function returnsVoid(): void {}

// this is allowed by the Hack typechecker and we will allow it too
takesAwaitableVoidFunction(
    async (): Awaitable<void> ==> {
        return returnsVoid();
    }
);

// this is allowed by the Hack typechecker and we will allow it too
takesAwaitableVoidFunction(
    async () ==> {
        return returnsVoid();
    }
);