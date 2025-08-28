abstract class A {
    public static async function fetch_data_async(): Awaitable<int> {
        return 42;
    }

    public static function fetch_data(): int {
        return Asio\join(self::fetch_data_async());
    }

    public static function fetch_data2(): int {
        return Asio\join(static::fetch_data_async());
    }
}

class B extends A {}

function caller(): int {
    return A::fetch_data();
}

async function async_caller(): Awaitable<int> {
    return A::fetch_data();
}

function caller2(): int {
    return B::fetch_data2();
}

async function async_caller2(): Awaitable<int> {
    return B::fetch_data2();
}