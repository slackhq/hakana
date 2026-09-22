<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
function logger(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $s): void {}
$e = new Exception('outer', 0, new Exception(secret()));
logger($e->getPrevious()?->getMessage() ?? '');
