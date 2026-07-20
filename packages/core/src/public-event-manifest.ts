export * as PublicEventManifest from "./public-event-manifest"

import { Event } from "@kxen/schema/event"
import { EventManifest } from "@kxen/schema/event-manifest"

export const Definitions = EventManifest.ServerDefinitions
export const Latest = Event.latest(Definitions)
