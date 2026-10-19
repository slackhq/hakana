function fetch(<<Hakana\SecurityAnalysis\Sink('UnauthorizedDataFetchKey')>> int $id): void {}
fetch((int)HH\global_get('_GET')['id']);
