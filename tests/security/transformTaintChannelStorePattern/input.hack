function decode_channel(
    <<\Hakana\SecurityAnalysis\PropagateTaint>> string $id,
): int {
    return 123;
}

class Channel {
    private dict<string, string> $channel_row;

    public function __construct(dict<string, string> $row) {
        $this->channel_row = $row;
    }

    public function getName(): string {
        return $this->channel_row['name'];
    }
}

function get_connection(): AsyncMysqlConnection {}

class ChannelStore {
    public static function fetchById(
        <<\Hakana\SecurityAnalysis\TransformTaint>> int $channel_id,
    ): Channel {
        $name = $conn->query("select name from channels where id = " . $channel_id)->dictRowsTyped()[0]['name'];
        return new Channel(dict['name' => $name]);
    }
}

function render(
    <<\Hakana\SecurityAnalysis\Sink('Output')>> string $html,
): void {}

function handleRequest(): void {
    $encoded_channel_id = (string) HH\global_get('_POST')['channel_id'];
    $channel_id = decode_channel($encoded_channel_id);
    $channel = ChannelStore::fetchById($channel_id);
    render($channel->getName());
}
