<<Hakana\SecurityAnalysis\Source('UserData')>>
function private_value(): string { return 'private'; }

function log_value(<<Hakana\SecurityAnalysis\Sink('Logging')>> string $value): void {}

abstract class Visitor<T> {
    public function visit(): T { return $this->wrapped(); }
    protected function wrapped(): T { return $this->item(); }
    abstract protected function item(): T;
}

final class TextVisitor extends Visitor<string> {
    protected function item(): string { return private_value(); }
}

final class ShapeVisitor extends Visitor<shape('id' => string)> {
    protected function item(): shape('id' => string) { return shape('id' => 'public'); }
}

(new TextVisitor())->visit();
log_value((new ShapeVisitor())->visit()['id']);
