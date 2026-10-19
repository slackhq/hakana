<<Hakana\SecurityAnalysis\Source('SystemSecret')>>
function secret(): string { return ''; }
class DomainError extends Exception {
    public function __construct(string $message) { parent::__construct('redacted'); }
}
function logger(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $s): void {}
logger((new DomainError(secret()))->getMessage());
