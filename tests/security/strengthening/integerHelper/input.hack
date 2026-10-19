function get_id(): int { return (int)HH\global_get('_POST')['id']; }
function pass(int $id): int { return $id; }
function fetch(<<Hakana\SecurityAnalysis\Sink('UnauthorizedDataFetchKey')>> int $id): void {}
fetch(pass(get_id()));
