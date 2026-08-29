/**
 * @typedef {object} El
 * @property {string} id
 * @property {string} class
 */
/**
 * @typedef {object & El} Parent
 * @property {Elem[]} children
 */
/**
 * @typedef {object & Parent} P
 */
/**
 * @typedef {object & El} Ul
 * @property {Li[]} children
 */
/**
 * @typedef {object & Parent} Li
 */
/**
 * @typedef {object & Parent} Button
 */
/**
 * @typedef {object & Parent} Span
 */
/**
 * @typedef {P|Ul|Li|Button|Span} Elem
 */
/**
 * @template {Elem} T
 * @typedef {Partial<T> & {id: string}} Mut
 */
export class CustomWidget {
  /** @param {Elem} root */
  init(root) {

  }
  /** @param {Mut<Elem>[]} els */
  mutate(els) {
  }
}
