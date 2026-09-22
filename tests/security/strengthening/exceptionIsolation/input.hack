<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
function logger(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $s): void {}
$secret = new Exception(secret());
$public = new Exception('public');
logger($public->getMessage());
logger((string)$secret->getCode());
