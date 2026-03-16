function decode_channel(
    <<\Hakana\SecurityAnalysis\PropagateTaint>> string $id,
): int {
    return 123;
}

class ChannelStore {
    public static async function fetchById(
        <<\Hakana\SecurityAnalysis\Sink('UnauthorizedDataFetchKey')>> int $channel_id,
    ): Awaitable<void> {}
}

function handleRequest(): void {
    $encoded_channel_id = (string) HH\global_get('_POST')['channel_id'];
    $channel_id = decode_channel($encoded_channel_id);
    Asio\join(ChannelStore::fetchById($channel_id));
}
