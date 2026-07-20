export interface KxenEvent<T = unknown> {
	type: string;
	ts: number;
	payload: T;
}

export interface EventBusOptions {
	// 环形缓冲容量，超出丢弃最旧（E5 drop-oldest）
	capacity?: number;
	// 单个消费者积压上限，超出断开并记事件（慢消费者断开）
	slowConsumerLimit?: number;
	now?: () => number;
}

type Handler = (event: KxenEvent) => void | Promise<void>;

interface Subscriber {
	owner: object;
	handler: Handler;
	inFlight: number;
	dropped: boolean;
}

export class EventBus {
	private buffer: KxenEvent[] = [];
	private subscribers = new Map<number, Subscriber>();
	private nextId = 1;
	private readonly capacity: number;
	private readonly slowConsumerLimit: number;
	private readonly now: () => number;

	constructor(opts: EventBusOptions = {}) {
		this.capacity = opts.capacity ?? 10_000;
		this.slowConsumerLimit = opts.slowConsumerLimit ?? 1_000;
		this.now = opts.now ?? Date.now;
	}

	publish(type: string, payload: unknown): void {
		const event: KxenEvent = { type, ts: this.now(), payload };
		this.buffer.push(event);
		if (this.buffer.length > this.capacity) {
			this.buffer.splice(0, this.buffer.length - this.capacity);
		}
		for (const [id, sub] of this.subscribers) {
			if (sub.dropped) continue;
			if (sub.inFlight > this.slowConsumerLimit) {
				sub.dropped = true;
				this.subscribers.delete(id);
				this.buffer.push({
					type: 'eventbus.consumer_dropped',
					ts: this.now(),
					payload: {},
				});
				continue;
			}
			try {
				const result = sub.handler(event);
				if (result instanceof Promise) {
					sub.inFlight++;
					void result.finally(() => {
						sub.inFlight--;
					});
				}
			} catch {
				// handler 异常不阻塞其他消费者与发布方
			}
		}
	}

	subscribe(owner: object, handler: Handler): () => void {
		const id = this.nextId++;
		this.subscribers.set(id, { owner, handler, inFlight: 0, dropped: false });
		return () => {
			this.subscribers.delete(id);
		};
	}

	unsubscribeAll(owner: object): void {
		for (const [id, sub] of this.subscribers) {
			if (sub.owner === owner) this.subscribers.delete(id);
		}
	}

	recent(n?: number): readonly KxenEvent[] {
		return n === undefined ? this.buffer : this.buffer.slice(-n);
	}

	get size(): number {
		return this.buffer.length;
	}
}
