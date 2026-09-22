<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
function logger(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $s): void {}
$e = new Exception(secret());
logger($e->getMessage());
