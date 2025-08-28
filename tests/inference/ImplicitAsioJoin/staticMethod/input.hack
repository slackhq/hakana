async function external_data(): Awaitable<int> {
    return 42;
}

final class DataService {
    public static async function fetch_data_async(): Awaitable<int> {
        return await external_data();
    }

    // This sync static method just wraps the async version
    public static function fetch_data(): int {
        return Asio\join(self::fetch_data_async());
    }
}

function caller(): int {
    return DataService::fetch_data(); // This should trigger ImplicitAsioJoin
}

async function async_caller(): Awaitable<int> {
    $result = await external_data();
    return DataService::fetch_data() + $result; // This should also trigger but suggest await instead of Asio\join
}