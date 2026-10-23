export interface Options {
  verbose: boolean
}
export type Mode = "fast" | "slow"
export const defaults: Options = { verbose: false }
export function describe(mode: Mode, options: Options = defaults): string {
  return `${mode}:${options.verbose}`
}
