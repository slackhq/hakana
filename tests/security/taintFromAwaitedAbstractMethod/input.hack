abstract class Validator<+T> {
    <<__LateInit>> private T $input;

    <<\Hakana\SecurityAnalysis\Source('UriRequestHeader')>>
    public function getInput(): T {
        return $this->input;
    }
}

abstract class InputHandler<TArgs> {
    public Validator<TArgs> $validator;

    public function __construct(public Validator<TArgs> $validator) {}

    public function getValidatedInput() {
        $input = $this->validator->getInput();

        $this->handleResult($input);
    }

    async public function handleResult(TArgs $args): Awaitable<mixed> {
        $a = await $this->getResult($args);
        echo $a;
        return $a;
    }

    abstract public function getResult(TArgs $args): Awaitable<mixed>;
}

type my_args_t = shape('a' => string);

abstract class AHandler extends InputHandler<my_args_t> {}
abstract class BHandler extends InputHandler<my_args_t> {}
abstract class CHandler extends InputHandler<my_args_t> {}

class MyHandler extends BHandler {
    async public function getResult(my_args_t $args): Awaitable<string> {
        return $args['a'];
    }
}