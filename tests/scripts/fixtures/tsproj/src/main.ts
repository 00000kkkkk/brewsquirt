import { square } from "@lib/math"
import { fmt } from "utils/fmt"
import config from "#config"
export const report = (): string => `${config.name} ${fmt("square", square(7))}`
