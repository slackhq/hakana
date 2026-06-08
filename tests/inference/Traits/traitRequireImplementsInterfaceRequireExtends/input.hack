abstract class BaseTask {
	public function getTaskId(): string {
		return "id";
	}
}

interface SummaryTask {
	require extends BaseTask;

	public function getMetadata(): string;
}

trait SummaryAware {
	require implements SummaryTask;

	public function getDebugContext(): string {
		// getTaskId is declared on BaseTask, reachable via the required
		// interface's `require extends` — this previously caused an
		// infinite loop in the symbol-reference walk because the trait
		// has no parent-class chain leading to BaseTask
		return $this->getTaskId();
	}
}

final class ChannelSummaryTask extends BaseTask implements SummaryTask {
	use SummaryAware;

	<<__Override>>
	public function getMetadata(): string {
		return "metadata";
	}
}
