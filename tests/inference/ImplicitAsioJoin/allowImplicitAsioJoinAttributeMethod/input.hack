final class DataFetcher {
    async function get_external_data_async(): Awaitable<int> {
        return 42;
    }

    // This sync method wraps the async version but uses the attribute to prevent ImplicitAsioJoin error
    <<Hakana\AllowImplicitAsioJoin>>
    function get_external_data(): int {
        return Asio\join($this->get_external_data_async());
    }

    function caller(): int {
        return $this->get_external_data(); // This should NOT trigger ImplicitAsioJoin due to the attribute
    }

    async function async_caller(): Awaitable<int> {
        $result = await $this->get_external_data_async();
        return $this->get_external_data() + $result; // This should also NOT trigger ImplicitAsioJoin due to the attribute
    }
}