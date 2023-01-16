abstract class Validator<+T> {
    <<__LateInit>> private T $input;

    <<\Hakana\SecurityAnalysis\Source('NonUriRequestHeader')>>
    public function getInput(): T {
        return $this->input;
    }
}

abstract class InputHandler<TArgs> {
    public function __construct(public Validator<TArgs> $validator) {}

    public function getValidatedInput() {
        $input = $this->validator->getInput();

        $this->foo($input);
    }

    public function foo(TArgs $args) {
        $this->getResult($args);
    }

    abstract public function getResult(TArgs $args): void;
}

type my_args_t = shape('a' => string);

abstract class AHandler extends InputHandler<my_args_t> {}
abstract class BHandler extends InputHandler<my_args_t> {}
abstract class CHandler extends InputHandler<my_args_t> {}

class MyHandler extends BHandler {
    public function getResult(my_args_t $args) {
        B::handle($args);
    }
}

class B {
    public static function handle(my_args_t $args) {
        $ch = curl_init($args['a']);
    }
}