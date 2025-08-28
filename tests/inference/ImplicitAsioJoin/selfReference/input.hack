final class TestClass {
    public static async function async_method(): Awaitable<int> {
        await \HH\Asio\usleep(100000);
        return 42;
    }

    public static function sync_method(): int {
        return Asio\join(self::async_method()); // This should be detected by scanner
    } 
}

function caller(): int {
    return TestClass::sync_method(); // This should trigger ImplicitAsioJoin
}