interface Configurable {
	public function create(): string;
}

abstract class ConcreteBuilder implements Configurable {
	<<__Override>>
	public function create(): string {
		return "built";
	}

	public function addInput(string $input): ConcreteBuilder {
		return $this;
	}
}

abstract class BaseTask {
	abstract const type TBuilder as Configurable;

	public async function processBuilder(this::TBuilder $builder): Awaitable<this::TBuilder> {
		return $builder;
	}
}

trait RequiresBaseTask {
	require extends BaseTask;
}

abstract class SpecializedTask extends BaseTask {
	const type TBuilder = ConcreteBuilder;

	<<__Override>>
	public async function processBuilder(this::TBuilder $builder): Awaitable<this::TBuilder> {
		return $builder;
	}
}

final class FinalTask extends SpecializedTask {
	use RequiresBaseTask;

	<<__Override>>
	public async function processBuilder(this::TBuilder $builder): Awaitable<this::TBuilder> {
		// this::TBuilder should resolve to ConcreteBuilder (via SpecializedTask),
		// not Configurable (from BaseTask)
		return $builder->addInput("test");
	}
}

interface TaskInterface {
	require extends BaseTask;
}

async function doWork(TaskInterface $task, ConcreteBuilder $builder): Awaitable<void> {
	if ($task is SpecializedTask) {
		$builder = await $task->processBuilder($builder);
		$builder->addInput('a');
	}
}
