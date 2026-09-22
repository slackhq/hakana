namespace Api;
abstract class BaseRequest<T as shape(...)> {
public function __construct(private T $args) {}
<<\Hakana\SecurityAnalysis\Source('NonUriRequestHeader')>>
public function getArgs(): T { return $this->args; }
}
final class Request extends BaseRequest<shape('q' => string)> {}
function handle(Request $r): void { echo $r->getArgs()['q']; }
