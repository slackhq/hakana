<<__Sealed(MessageWithChecksum::class)>>
class Message {
    public function update(): int {
        return 1;
    }
}

final class MessageWithChecksum extends Message {}

function update_message(Message $message): int {
    $build_update = (int $update): int ==> $update;
    if ($message is MessageWithChecksum) {
        return $build_update($message->update());
    } else {
        $update = $build_update($message->update());
    }
    return $update;
}

<<__Sealed(FirstChild::class, SecondChild::class)>>
class Value<T> {
    public function __construct(private T $value) {}

    public function get(): T {
        return $this->value;
    }
}

final class FirstChild<T> extends Value<T> {}
final class SecondChild<T> extends Value<T> {}

function read_value(Value<bool> $value): bool {
    if ($value is FirstChild<_>) {
        return false;
    }
    if ($value is SecondChild<_>) {
        return false;
    }
    return $value->get();
}
