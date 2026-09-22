function fetch(<<Hakana\SecurityAnalysis\Sink('UnauthorizedDataFetchKey')>> int $id): void {}
$ids = tuple((int)HH\global_get('_GET')['id'], 0);
fetch($ids[0]);
