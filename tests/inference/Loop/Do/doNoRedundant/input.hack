final class Event {}

function fetchEvent(): ?Event {
    return rand(0, 1) ? new Event() : null;
}

function nextEvent(bool $c): void {
    do {
        $e = fetchEvent();
    } while ($c && $e);
}