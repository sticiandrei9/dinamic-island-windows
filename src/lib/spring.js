// Spring fizic (Euler semi-implicit cu sub-pași), parametrizat ca în SwiftUI:
// response = perioada în secunde, damping = fracția de amortizare (1 = fără depășire).
export class Spring {
  constructor(value, { response = 0.5, damping = 0.72 } = {}) {
    this.value = value;
    this.target = value;
    this.velocity = 0;
    const omega = (2 * Math.PI) / response;
    this.k = omega * omega;
    this.c = 2 * damping * omega;
  }

  setTarget(target) {
    this.target = target;
  }

  step(dt) {
    const sub = Math.max(1, Math.ceil(dt * 240));
    const h = dt / sub;
    for (let i = 0; i < sub; i++) {
      const force = -this.k * (this.value - this.target) - this.c * this.velocity;
      this.velocity += force * h;
      this.value += this.velocity * h;
    }
    return this.isSettled();
  }

  isSettled() {
    return Math.abs(this.value - this.target) < 0.05 && Math.abs(this.velocity) < 0.05;
  }

  snap() {
    this.value = this.target;
    this.velocity = 0;
  }
}
