<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
function logger(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $s): void {}
logger((string)mb_strlen(secret()));
